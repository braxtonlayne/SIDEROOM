//! Image pipeline (FND-STOR-003): decode, crop/resize, re-encode.
//!
//! Pure Rust (`image` crate, no libwebp). Used by the `/cdn` resizer
//! (`?size=`/`format=`) and by the avatar/banner/icon upload paths.
//!
//! Guarantees:
//! * **EXIF-free output.** Every output is freshly encoded from pixels, so no
//!   EXIF/XMP/IPTC/text chunk or ICC profile survives. The EXIF orientation is
//!   applied to the pixels first.
//! * **Aspect-preserving, never upscaled.** A size is the longest edge in
//!   pixels; a source smaller than the requested size is returned at its own
//!   size.
//! * **Bounded work.** Sources over [`Limits::max_pixels`] / [`Limits::max_bytes`]
//!   are refused with [`ImgError::TooLarge`] *before* pixels are decoded (the
//!   dimensions come from the header); animations are capped by frame count and
//!   total pixels.
//! * **Animation.** Animated GIF is resized frame by frame and stays an
//!   animated GIF. Animated WebP is decoded as its first frame only, and any
//!   animated source asked for as PNG/JPEG/WebP yields its first frame: the
//!   `image` crate's WebP encoder is lossless and still-only.
//! * **WebP is lossless.** `image`'s pure-Rust encoder has no lossy mode; WebP
//!   output is therefore lossless (photographic sources are better served as
//!   JPEG, which [`pick_format`] prefers for JPEG sources).

use std::io::Cursor;

use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{
    AnimationDecoder, DynamicImage, ExtendedColorType, Frame, ImageDecoder, ImageFormat,
    ImageReader, Limits as DecodeLimits, RgbImage, RgbaImage,
};
use thiserror::Error;

/// Smallest `?size=`.
pub const MIN_SIZE: u32 = 16;
/// Largest `?size=`.
pub const MAX_SIZE: u32 = 4096;
const JPEG_QUALITY: u8 = 88;

/// Why an image could not be processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ImgError {
    /// Not a PNG, JPEG, GIF or WebP.
    #[error("unsupported image format")]
    Unsupported,
    /// Truncated or corrupt.
    #[error("malformed image data")]
    Malformed,
    /// Over the byte, pixel or frame budget.
    #[error("image exceeds the processing budget")]
    TooLarge,
    /// `size` is not a power of two in 16..=4096.
    #[error("size must be a power of two between 16 and 4096")]
    BadSize,
}

/// Processing budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest source file.
    pub max_bytes: u64,
    /// Largest `width * height` of one frame.
    pub max_pixels: u64,
    /// Largest `width * height * frames` of an animation.
    pub max_animation_pixels: u64,
    /// Most animation frames.
    pub max_frames: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_pixels: 24_000_000,
            max_animation_pixels: 32_000_000,
            max_frames: 400,
        }
    }
}

/// Output / input formats handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImgFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
}

impl ImgFormat {
    /// File extension used in derived keys and URLs.
    pub const fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Gif => "gif",
        }
    }

    /// `Content-Type`.
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
        }
    }

    /// Parse `png`, `jpg`, `jpeg`, `webp`, `gif` (case-insensitive).
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "webp" => Self::Webp,
            "gif" => Self::Gif,
            _ => return None,
        })
    }

    /// Sniff from magic bytes.
    pub fn sniff(data: &[u8]) -> Option<Self> {
        match crate::upload::sniff_mime(data)? {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            "image/gif" => Some(Self::Gif),
            "image/webp" => Some(Self::Webp),
            _ => None,
        }
    }

    const fn codec(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Webp => ImageFormat::WebP,
            Self::Gif => ImageFormat::Gif,
        }
    }
}

/// True for a power of two in 16..=4096.
pub fn valid_size(size: u32) -> bool {
    (MIN_SIZE..=MAX_SIZE).contains(&size) && size.is_power_of_two()
}

/// Object key of a derived variant: `<key>@<size>.<ext>` (`<key>@orig.<ext>`
/// when only the format changes).
pub fn variant_key(key: &str, size: Option<u32>, format: ImgFormat) -> String {
    match size {
        Some(s) => format!("{key}@{s}.{}", format.ext()),
        None => format!("{key}@orig.{}", format.ext()),
    }
}

/// Choose the output format. Order: explicit `requested` (`format=` or URL
/// extension), else keep the source's own format family:
/// animated GIF stays GIF, JPEG stays JPEG, otherwise WebP when the client
/// `accepts_webp`, else PNG.
pub fn pick_format(
    requested: Option<ImgFormat>,
    source: ImgFormat,
    animated: bool,
    accepts_webp: bool,
) -> ImgFormat {
    if let Some(f) = requested {
        return f;
    }
    match source {
        ImgFormat::Gif if animated => ImgFormat::Gif,
        ImgFormat::Jpeg => ImgFormat::Jpeg,
        _ if accepts_webp => ImgFormat::Webp,
        _ => ImgFormat::Png,
    }
}

/// Header-level facts about a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    pub format: ImgFormat,
    pub width: u32,
    pub height: u32,
}

/// Read the format and dimensions from the header without decoding pixels, and
/// enforce the byte and pixel budget.
pub fn probe(src: &[u8], limits: &Limits) -> Result<Probe, ImgError> {
    if src.len() as u64 > limits.max_bytes {
        return Err(ImgError::TooLarge);
    }
    let format = ImgFormat::sniff(src).ok_or(ImgError::Unsupported)?;
    let (width, height) = ImageReader::with_format(Cursor::new(src), format.codec())
        .into_dimensions()
        .map_err(|_| ImgError::Malformed)?;
    if width == 0 || height == 0 {
        return Err(ImgError::Malformed);
    }
    if u64::from(width) * u64::from(height) > limits.max_pixels {
        return Err(ImgError::TooLarge);
    }
    Ok(Probe {
        format,
        width,
        height,
    })
}

/// A decoded image: one still, or an animation.
pub enum Decoded {
    Still(DynamicImage),
    Anim(Vec<Frame>),
}

impl Decoded {
    /// More than one frame.
    pub fn is_animated(&self) -> bool {
        matches!(self, Self::Anim(f) if f.len() > 1)
    }

    /// Width and height of the (first) frame.
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Still(i) => (i.width(), i.height()),
            Self::Anim(f) => f
                .first()
                .map_or((0, 0), |f| (f.buffer().width(), f.buffer().height())),
        }
    }
}

fn decode_limits(limits: &Limits) -> DecodeLimits {
    let mut l = DecodeLimits::default();
    // RGBA8 of the largest permitted frame, with headroom for the decoder.
    l.max_alloc = Some(limits.max_pixels.saturating_mul(8).max(64 * 1024 * 1024));
    l
}

/// Decode `src` within the budget. Animated GIFs keep their frames (a
/// single-frame GIF becomes a still); animated WebP gives its first frame.
pub fn decode(src: &[u8], limits: &Limits) -> Result<(Probe, Decoded), ImgError> {
    let p = probe(src, limits)?;
    if p.format == ImgFormat::Gif {
        let dec = GifDecoder::new(Cursor::new(src)).map_err(|_| ImgError::Malformed)?;
        let per_frame = u64::from(p.width) * u64::from(p.height);
        let mut frames: Vec<Frame> = Vec::new();
        for f in dec.into_frames() {
            let f = f.map_err(|_| ImgError::Malformed)?;
            if frames.len() + 1 > limits.max_frames
                || per_frame * (frames.len() as u64 + 1) > limits.max_animation_pixels
            {
                return Err(ImgError::TooLarge);
            }
            frames.push(f);
        }
        if frames.is_empty() {
            return Err(ImgError::Malformed);
        }
        if frames.len() == 1 {
            let f = frames.remove(0);
            return Ok((p, Decoded::Still(DynamicImage::ImageRgba8(f.into_buffer()))));
        }
        return Ok((p, Decoded::Anim(frames)));
    }
    let mut dec = ImageReader::with_format(Cursor::new(src), p.format.codec())
        .into_decoder()
        .map_err(|_| ImgError::Malformed)?;
    dec.set_limits(decode_limits(limits))
        .map_err(|_| ImgError::TooLarge)?;
    let orientation = dec.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(dec).map_err(|_| ImgError::Malformed)?;
    img.apply_orientation(orientation);
    Ok((p, Decoded::Still(img)))
}

/// A geometric operation: optional centred square crop, then fit inside
/// `max_w x max_h` keeping the aspect ratio, never upscaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    pub square: bool,
    pub max_w: u32,
    pub max_h: u32,
}

impl Op {
    /// Longest edge `size`, no crop (the `?size=` semantics).
    pub const fn fit(size: u32) -> Self {
        Self {
            square: false,
            max_w: size,
            max_h: size,
        }
    }
}

fn target_dims(w: u32, h: u32, op: Op) -> (u32, u32) {
    let (w, h) = if op.square {
        let s = w.min(h);
        (s, s)
    } else {
        (w, h)
    };
    let scale = (f64::from(op.max_w) / f64::from(w))
        .min(f64::from(op.max_h) / f64::from(h))
        .min(1.0);
    if scale >= 1.0 {
        return (w, h);
    }
    (
        ((f64::from(w) * scale).round() as u32).max(1),
        ((f64::from(h) * scale).round() as u32).max(1),
    )
}

fn crop_square(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w == h {
        return img.clone();
    }
    let s = w.min(h);
    image::imageops::crop_imm(img, (w - s) / 2, (h - s) / 2, s, s).to_image()
}

fn transform_rgba(img: &RgbaImage, op: Op, filter: FilterType) -> RgbaImage {
    let cropped;
    let src = if op.square && img.width() != img.height() {
        cropped = crop_square(img);
        &cropped
    } else {
        img
    };
    let (tw, th) = target_dims(img.width(), img.height(), op);
    if (tw, th) == src.dimensions() {
        return src.clone();
    }
    image::imageops::resize(src, tw, th, filter)
}

/// Apply `op` to every frame.
pub fn transform(d: Decoded, op: Op) -> Decoded {
    match d {
        Decoded::Still(img) => {
            let (w, h) = (img.width(), img.height());
            let (tw, th) = target_dims(w, h, op);
            if !op.square && (tw, th) == (w, h) {
                return Decoded::Still(img);
            }
            Decoded::Still(DynamicImage::ImageRgba8(transform_rgba(
                &img.to_rgba8(),
                op,
                FilterType::Lanczos3,
            )))
        }
        Decoded::Anim(frames) => Decoded::Anim(
            frames
                .into_iter()
                .map(|f| {
                    let delay = f.delay();
                    Frame::from_parts(
                        transform_rgba(f.buffer(), op, FilterType::Triangle),
                        0,
                        0,
                        delay,
                    )
                })
                .collect(),
        ),
    }
}

fn flatten_white(img: &RgbaImage) -> RgbImage {
    let mut out = RgbImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        let a = u32::from(p[3]);
        let blend = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
        out.put_pixel(x, y, image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])]));
    }
    out
}

/// Encode as `format`. An animation is kept only for GIF; other formats get
/// the first frame.
pub fn encode(d: &Decoded, format: ImgFormat) -> Result<Vec<u8>, ImgError> {
    let mut out = Vec::new();
    match (d, format) {
        (Decoded::Anim(frames), ImgFormat::Gif) => {
            let mut enc = GifEncoder::new_with_speed(&mut out, 10);
            enc.set_repeat(Repeat::Infinite)
                .map_err(|_| ImgError::Malformed)?;
            enc.encode_frames(frames.iter().cloned())
                .map_err(|_| ImgError::Malformed)?;
        }
        _ => {
            let rgba: RgbaImage = match d {
                Decoded::Still(i) => i.to_rgba8(),
                Decoded::Anim(f) => f.first().ok_or(ImgError::Malformed)?.buffer().clone(),
            };
            let (w, h) = rgba.dimensions();
            match format {
                ImgFormat::Png => {
                    DynamicImage::ImageRgba8(rgba)
                        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
                        .map_err(|_| ImgError::Malformed)?;
                }
                ImgFormat::Gif => {
                    DynamicImage::ImageRgba8(rgba)
                        .write_to(&mut Cursor::new(&mut out), ImageFormat::Gif)
                        .map_err(|_| ImgError::Malformed)?;
                }
                ImgFormat::Jpeg => {
                    let rgb = flatten_white(&rgba);
                    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
                        .encode(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)
                        .map_err(|_| ImgError::Malformed)?;
                }
                ImgFormat::Webp => {
                    image::codecs::webp::WebPEncoder::new_lossless(&mut out)
                        .encode(rgba.as_raw(), w, h, ExtendedColorType::Rgba8)
                        .map_err(|_| ImgError::Malformed)?;
                }
            }
        }
    }
    Ok(out)
}

/// A rendered variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub bytes: Vec<u8>,
    pub format: ImgFormat,
    pub width: u32,
    pub height: u32,
    pub animated: bool,
}

/// Render `src` at longest edge `size` (or its own size when `None`) as
/// `format`. `size` must satisfy [`valid_size`].
pub fn render(
    src: &[u8],
    size: Option<u32>,
    format: ImgFormat,
    limits: &Limits,
) -> Result<Rendered, ImgError> {
    if size.is_some_and(|s| !valid_size(s)) {
        return Err(ImgError::BadSize);
    }
    let (_, decoded) = decode(src, limits)?;
    let decoded = transform(decoded, Op::fit(size.unwrap_or(MAX_SIZE)));
    finish(&decoded, format)
}

fn finish(d: &Decoded, format: ImgFormat) -> Result<Rendered, ImgError> {
    let (width, height) = d.dimensions();
    Ok(Rendered {
        bytes: encode(d, format)?,
        format,
        width,
        height,
        animated: d.is_animated() && format == ImgFormat::Gif,
    })
}

/// How an uploaded image is normalised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Centred square crop, at most `max` px a side (avatars, icons).
    Square { max: u32 },
    /// Keep the aspect ratio, fit inside `w x h` (banners, splashes).
    Bound { w: u32, h: u32 },
}

/// A normalised upload, ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub bytes: Vec<u8>,
    pub format: ImgFormat,
    pub animated: bool,
    pub width: u32,
    pub height: u32,
}

/// Validate, strip and normalise an uploaded image: content-sniffed, EXIF
/// removed by re-encoding, cropped/bounded per `fit`. JPEG stays JPEG, an
/// animated GIF stays an animated GIF, everything else becomes PNG.
pub fn normalize(src: &[u8], fit: Fit, limits: &Limits) -> Result<Normalized, ImgError> {
    let (probe, decoded) = decode(src, limits)?;
    let op = match fit {
        Fit::Square { max } => Op {
            square: true,
            max_w: max,
            max_h: max,
        },
        Fit::Bound { w, h } => Op {
            square: false,
            max_w: w,
            max_h: h,
        },
    };
    let decoded = transform(decoded, op);
    let format = if decoded.is_animated() {
        ImgFormat::Gif
    } else if probe.format == ImgFormat::Jpeg {
        ImgFormat::Jpeg
    } else {
        ImgFormat::Png
    };
    let r = finish(&decoded, format)?;
    Ok(Normalized {
        bytes: r.bytes,
        format,
        animated: r.animated,
        width: r.width,
        height: r.height,
    })
}

#[cfg(test)]
mod tests;
