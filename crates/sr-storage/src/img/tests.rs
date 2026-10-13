use super::*;
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Rgba};

fn solid(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255])
    })
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(solid(w, h))
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

fn jpeg_with_exif(w: u32, h: u32) -> Vec<u8> {
    let rgb = DynamicImage::ImageRgba8(solid(w, h)).to_rgb8();
    let mut j = Vec::new();
    JpegEncoder::new_with_quality(&mut j, 90)
        .encode(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)
        .unwrap();
    // Splice an APP1 "Exif" segment after SOI.
    let payload = b"Exif\0\0GPSLATITUDE-SECRET";
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&j[2..]);
    out
}

fn animated_gif(w: u32, h: u32, n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut out);
        enc.set_repeat(Repeat::Infinite).unwrap();
        for i in 0..n {
            let img = RgbaImage::from_fn(w, h, |x, _| {
                if (x as usize / 4 + i).is_multiple_of(2) {
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

fn dims(bytes: &[u8]) -> (u32, u32) {
    let r = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .unwrap();
    r.into_dimensions().unwrap()
}

#[test]
fn size_validation() {
    for s in [16, 32, 64, 128, 256, 512, 1024, 2048, 4096] {
        assert!(valid_size(s), "{s}");
    }
    for s in [0, 8, 15, 17, 100, 4097, 8192, 3000] {
        assert!(!valid_size(s), "{s}");
    }
    let r = render(&png(8, 8), Some(100), ImgFormat::Png, &Limits::default());
    assert_eq!(r.unwrap_err(), ImgError::BadSize);
}

#[test]
fn resizes_keeping_aspect_and_never_upscales() {
    let src = png(400, 200);
    let l = Limits::default();
    let r = render(&src, Some(64), ImgFormat::Png, &l).unwrap();
    assert_eq!((r.width, r.height), (64, 32));
    assert_eq!(dims(&r.bytes), (64, 32));
    // Larger than the source: unchanged dimensions.
    let r = render(&src, Some(1024), ImgFormat::Png, &l).unwrap();
    assert_eq!((r.width, r.height), (400, 200));
    // No size: original dimensions.
    let r = render(&src, None, ImgFormat::Webp, &l).unwrap();
    assert_eq!((r.width, r.height), (400, 200));
}

#[test]
fn outputs_every_format() {
    let src = png(100, 100);
    let l = Limits::default();
    for (f, magic) in [
        (ImgFormat::Png, &b"\x89PNG"[..]),
        (ImgFormat::Jpeg, &[0xFF, 0xD8, 0xFF][..]),
        (ImgFormat::Gif, &b"GIF8"[..]),
        (ImgFormat::Webp, &b"RIFF"[..]),
    ] {
        let r = render(&src, Some(32), f, &l).unwrap();
        assert!(r.bytes.starts_with(magic), "{f:?}");
        assert_eq!(ImgFormat::sniff(&r.bytes), Some(f));
        assert_eq!(dims(&r.bytes), (32, 32));
    }
}

#[test]
fn pick_format_rules() {
    use ImgFormat::*;
    assert_eq!(pick_format(Some(Png), Jpeg, false, true), Png);
    assert_eq!(pick_format(None, Gif, true, true), Gif);
    assert_eq!(pick_format(None, Gif, false, true), Webp);
    assert_eq!(pick_format(None, Jpeg, false, true), Jpeg);
    assert_eq!(pick_format(None, Png, false, false), Png);
    assert_eq!(pick_format(None, Png, false, true), Webp);
    assert_eq!(ImgFormat::from_name("JPG"), Some(Jpeg));
    assert_eq!(ImgFormat::from_name("avif"), None);
}

#[test]
fn variant_keys() {
    assert_eq!(
        variant_key("avatars/1/abc", Some(128), ImgFormat::Webp),
        "avatars/1/abc@128.webp"
    );
    assert_eq!(
        variant_key("chat/1/2/a.png", None, ImgFormat::Jpeg),
        "chat/1/2/a.png@orig.jpg"
    );
}

#[test]
fn exif_is_gone_after_render_and_normalize() {
    let src = jpeg_with_exif(64, 48);
    assert!(src.windows(11).any(|w| w == b"GPSLATITUDE"));
    let l = Limits::default();
    for f in [ImgFormat::Jpeg, ImgFormat::Png, ImgFormat::Webp] {
        let r = render(&src, Some(32), f, &l).unwrap();
        assert!(!r.bytes.windows(11).any(|w| w == b"GPSLATITUDE"));
        assert!(!r.bytes.windows(4).any(|w| w == b"Exif"));
    }
    let n = normalize(&src, Fit::Square { max: 1024 }, &l).unwrap();
    assert_eq!(n.format, ImgFormat::Jpeg);
    assert!(!n.bytes.windows(4).any(|w| w == b"Exif"));
}

#[test]
fn pixel_budget_is_enforced_before_decoding() {
    let l = Limits {
        max_pixels: 1000,
        ..Limits::default()
    };
    let big = png(100, 100);
    assert_eq!(probe(&big, &l).unwrap_err(), ImgError::TooLarge);
    assert_eq!(
        render(&big, Some(16), ImgFormat::Png, &l).unwrap_err(),
        ImgError::TooLarge
    );
    let l = Limits {
        max_bytes: 10,
        ..Limits::default()
    };
    assert_eq!(probe(&big, &l).unwrap_err(), ImgError::TooLarge);
}

#[test]
fn rejects_non_images() {
    let l = Limits::default();
    assert_eq!(
        probe(b"<svg xmlns=...>", &l).unwrap_err(),
        ImgError::Unsupported
    );
    assert_eq!(probe(b"%PDF-1.4", &l).unwrap_err(), ImgError::Unsupported);
    let mut cut = png(50, 50);
    cut.truncate(40);
    assert!(render(&cut, Some(16), ImgFormat::Png, &l).is_err());
}

#[test]
fn animated_gif_stays_animated() {
    let src = animated_gif(64, 32, 4);
    let l = Limits::default();
    let (_, d) = decode(&src, &l).unwrap();
    assert!(d.is_animated());
    let r = render(&src, Some(16), ImgFormat::Gif, &l).unwrap();
    assert!(r.animated);
    assert_eq!((r.width, r.height), (16, 8));
    let (_, d2) = decode(&r.bytes, &l).unwrap();
    match d2 {
        Decoded::Anim(f) => assert_eq!(f.len(), 4),
        Decoded::Still(_) => panic!("lost the animation"),
    }
    // Asked for PNG: first frame only.
    let r = render(&src, Some(16), ImgFormat::Png, &l).unwrap();
    assert!(!r.animated);
    assert_eq!(ImgFormat::sniff(&r.bytes), Some(ImgFormat::Png));
}

#[test]
fn animation_frame_budget() {
    let src = animated_gif(32, 32, 6);
    let l = Limits {
        max_frames: 3,
        ..Limits::default()
    };
    assert_eq!(decode(&src, &l).err(), Some(ImgError::TooLarge));
    let l = Limits {
        max_animation_pixels: 32 * 32 * 2,
        ..Limits::default()
    };
    assert_eq!(decode(&src, &l).err(), Some(ImgError::TooLarge));
}

#[test]
fn normalize_square_and_bound() {
    let l = Limits::default();
    let n = normalize(&png(300, 200), Fit::Square { max: 128 }, &l).unwrap();
    assert_eq!((n.width, n.height), (128, 128));
    assert_eq!(n.format, ImgFormat::Png);
    // Small source: cropped to the short side, not upscaled.
    let n = normalize(&png(300, 200), Fit::Square { max: 1024 }, &l).unwrap();
    assert_eq!((n.width, n.height), (200, 200));
    let n = normalize(&png(4000, 1000), Fit::Bound { w: 1920, h: 1080 }, &l).unwrap();
    assert_eq!((n.width, n.height), (1920, 480));
    let g = normalize(&animated_gif(40, 20, 3), Fit::Square { max: 1024 }, &l).unwrap();
    assert!(g.animated);
    assert_eq!(g.format, ImgFormat::Gif);
    assert_eq!((g.width, g.height), (20, 20));
}

#[test]
fn webp_source_decodes() {
    let l = Limits::default();
    let w = render(&png(40, 40), None, ImgFormat::Webp, &l).unwrap();
    let r = render(&w.bytes, Some(16), ImgFormat::Png, &l).unwrap();
    assert_eq!((r.width, r.height), (16, 16));
    let n = normalize(&w.bytes, Fit::Square { max: 1024 }, &l).unwrap();
    assert_eq!(n.format, ImgFormat::Png);
}
