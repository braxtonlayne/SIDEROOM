//! Video to GIF (UTL-IMG-001): the encoder behind the drawer's GIF maker.
//!
//! The browser hands over RGBA frames (read from a `<video>` through a canvas); this module builds the 256-colour
//! palette and writes the file with the `gif` crate's encoder. The quantiser is our own NeuQuant-style
//! self-organising map (`gif`'s own quantiser feature is off). When the frames use 256 colours or fewer in
//! total, the palette is exact and the round trip is lossless.
//!
//! Limits, enforced here and not only in the UI: the clip is at most 15 s ([`MAX_DURATION_MS`]) and the file at
//! most 20 MiB ([`MAX_BYTES`]).

use std::collections::{HashMap, HashSet};
use wasm_bindgen::prelude::*;

/// Longest clip: the sum of the frame delays.
pub const MAX_DURATION_MS: u32 = 15_000;
/// Largest output file.
pub const MAX_BYTES: usize = 20 * 1024 * 1024;
/// Largest frame edge.
pub const MAX_EDGE: u16 = 2048;
/// Most RGBA bytes held while frames are collected.
pub const MAX_BUFFERED: usize = 256 * 1024 * 1024;
/// Smallest frame delay in hundredths of a second (browsers show anything lower as 10).
pub const MIN_DELAY_CS: u16 = 2;
const MAX_DELAY_CS: u16 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GifError {
    /// Width or height is 0 or above [`MAX_EDGE`].
    BadSize,
    /// A frame is not `width * height * 4` bytes.
    BadFrame,
    /// No frames were added.
    Empty,
    /// The frames add up to more than 15 s.
    TooLong,
    /// The encoded file is larger than 20 MiB.
    TooBig,
    /// More pixel data than the encoder will hold.
    TooMuch,
    /// The `gif` crate failed to write.
    Encode,
}

impl GifError {
    /// A stable code for the UI to translate.
    pub fn code(self) -> &'static str {
        match self {
            Self::BadSize => "GIF_BAD_SIZE",
            Self::BadFrame => "GIF_BAD_FRAME",
            Self::Empty => "GIF_EMPTY",
            Self::TooLong => "GIF_TOO_LONG",
            Self::TooBig => "GIF_TOO_BIG",
            Self::TooMuch => "GIF_TOO_MUCH",
            Self::Encode => "GIF_ENCODE",
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Quantiser
// ---------------------------------------------------------------------------------------------------------

const COLOURS: usize = 256;
/// Training samples per pass (a stride sample of the pixels).
const SAMPLES: usize = 60_000;
const PASSES: usize = 3;
const LLOYD: usize = 6;

type Rgb = [f32; 3];

fn d2(a: &Rgb, b: &Rgb) -> f32 {
    let (x, y, z) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    x * x + y * y + z * z
}

fn to_f(c: [u8; 3]) -> Rgb {
    [f32::from(c[0]), f32::from(c[1]), f32::from(c[2])]
}

fn to_u8(n: &Rgb) -> [u8; 3] {
    [
        n[0].round().clamp(0.0, 255.0) as u8,
        n[1].round().clamp(0.0, 255.0) as u8,
        n[2].round().clamp(0.0, 255.0) as u8,
    ]
}

/// Train a 256-neuron self-organising map on `pixels`. Neurons start on the grey diagonal, so neighbours in
/// index order are neighbours in colour; the learning rate and the neighbourhood both shrink, which leaves each
/// neuron on a distinct cluster of the image's colours.
fn neuquant(pixels: &[[u8; 3]]) -> Vec<[u8; 3]> {
    let mut net: Vec<Rgb> = (0..COLOURS)
        .map(|i| {
            let v = i as f32 * 255.0 / (COLOURS as f32 - 1.0);
            [v, v, v]
        })
        .collect();
    if pixels.is_empty() {
        return net.iter().map(to_u8).collect();
    }
    let stride = pixels.len().div_ceil(SAMPLES).max(1);
    let sample: Vec<Rgb> = pixels.iter().step_by(stride).map(|p| to_f(*p)).collect();
    let total = (sample.len() * PASSES) as f32;
    let mut step = 0.0f32;
    for _ in 0..PASSES {
        for p in &sample {
            let progress = step / total;
            step += 1.0;
            let alpha = 0.5 * (1.0 - progress).powi(2) + 0.01;
            let radius = (24.0 * (1.0 - progress)) as isize;
            let (mut best, mut best_d) = (0usize, f32::MAX);
            for (i, n) in net.iter().enumerate() {
                let d = d2(n, p);
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            let lo = (best as isize - radius).max(0) as usize;
            let hi = ((best as isize + radius) as usize).min(COLOURS - 1);
            for (j, n) in net.iter_mut().enumerate().take(hi + 1).skip(lo) {
                let off = (j as isize - best as isize).abs() as f32;
                let a = if radius == 0 {
                    if j == best { alpha } else { 0.0 }
                } else {
                    alpha * (1.0 - (off * off) / ((radius * radius) as f32 + 1.0))
                };
                if a > 0.0 {
                    for c in 0..3 {
                        n[c] += a * (p[c] - n[c]);
                    }
                }
            }
        }
    }
    // A few Lloyd steps settle the neurons on the centres of the pixels they own.
    for _ in 0..LLOYD {
        let mut sums = vec![[0.0f32; 3]; COLOURS];
        let mut n = vec![0u32; COLOURS];
        for p in &sample {
            let (mut best, mut best_d) = (0usize, f32::MAX);
            for (i, q) in net.iter().enumerate() {
                let d = d2(q, p);
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            for c in 0..3 {
                sums[best][c] += p[c];
            }
            n[best] += 1;
        }
        for (i, q) in net.iter_mut().enumerate() {
            if n[i] > 0 {
                *q = [
                    sums[i][0] / n[i] as f32,
                    sums[i][1] / n[i] as f32,
                    sums[i][2] / n[i] as f32,
                ];
            }
        }
    }
    net.iter().map(to_u8).collect()
}

/// The palette for all frames: exact when they use 256 colours or fewer, trained otherwise. Always 256 entries.
fn build_palette(frames: &[Vec<u8>]) -> (Vec<[u8; 3]>, bool) {
    let mut exact: Vec<[u8; 3]> = Vec::new();
    let mut seen: HashSet<[u8; 3]> = HashSet::new();
    let mut overflow = false;
    'outer: for f in frames {
        for p in f.as_chunks::<4>().0.iter() {
            let c = [p[0], p[1], p[2]];
            if seen.insert(c) {
                exact.push(c);
                if exact.len() > COLOURS {
                    overflow = true;
                    break 'outer;
                }
            }
        }
    }
    if !overflow {
        exact.resize(COLOURS, [0, 0, 0]);
        return (exact, true);
    }
    let all: Vec<[u8; 3]> = frames
        .iter()
        .flat_map(|f| f.as_chunks::<4>().0.iter().map(|p| [p[0], p[1], p[2]]))
        .collect();
    (neuquant(&all), false)
}

fn nearest(palette: &[[u8; 3]], c: [u8; 3]) -> u8 {
    let target = to_f(c);
    let mut best = 0u8;
    let mut best_d = f32::MAX;
    for (i, q) in palette.iter().enumerate() {
        let d = d2(&to_f(*q), &target);
        if d < best_d {
            best_d = d;
            best = i as u8;
            if d == 0.0 {
                break;
            }
        }
    }
    best
}

/// Maps pixels to palette indices. An exact palette is looked up directly; a trained one is searched once per
/// 6-bit-per-channel bucket (the first pixel seen in the bucket stands for it), which keeps long clips fast.
struct Mapper<'a> {
    palette: &'a [[u8; 3]],
    exact: HashMap<[u8; 3], u8>,
    buckets: Vec<u16>,
    is_exact: bool,
}

impl<'a> Mapper<'a> {
    fn new(palette: &'a [[u8; 3]], is_exact: bool) -> Self {
        let exact = if is_exact {
            palette
                .iter()
                .enumerate()
                .rev()
                .map(|(i, c)| (*c, i as u8))
                .collect()
        } else {
            HashMap::new()
        };
        let buckets = if is_exact {
            Vec::new()
        } else {
            vec![u16::MAX; 1 << 18]
        };
        Mapper {
            palette,
            exact,
            buckets,
            is_exact,
        }
    }

    fn index(&mut self, p: &[u8]) -> u8 {
        let c = [p[0], p[1], p[2]];
        if self.is_exact {
            return self.exact.get(&c).copied().unwrap_or(0);
        }
        let key =
            (usize::from(c[0] >> 2) << 12) | (usize::from(c[1] >> 2) << 6) | usize::from(c[2] >> 2);
        if self.buckets[key] == u16::MAX {
            self.buckets[key] = u16::from(nearest(self.palette, c));
        }
        self.buckets[key] as u8
    }
}

// ---------------------------------------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------------------------------------

/// Encode `frames` (each `width * height * 4` RGBA bytes; alpha is ignored, the caller flattens onto a
/// background) as a looping GIF with `delay_cs` hundredths of a second per frame.
pub fn encode(
    width: u16,
    height: u16,
    delay_cs: u16,
    frames: &[Vec<u8>],
) -> Result<Vec<u8>, GifError> {
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return Err(GifError::BadSize);
    }
    if frames.is_empty() {
        return Err(GifError::Empty);
    }
    let want = usize::from(width) * usize::from(height) * 4;
    if frames.iter().any(|f| f.len() != want) {
        return Err(GifError::BadFrame);
    }
    let delay_cs = delay_cs.clamp(MIN_DELAY_CS, MAX_DELAY_CS);
    if frames.len() as u64 * u64::from(delay_cs) * 10 > u64::from(MAX_DURATION_MS) {
        return Err(GifError::TooLong);
    }

    let (palette, is_exact) = build_palette(frames);
    let flat: Vec<u8> = palette.iter().flatten().copied().collect();
    let mut out = Vec::new();
    {
        let mut enc =
            gif::Encoder::new(&mut out, width, height, &flat).map_err(|_| GifError::Encode)?;
        enc.set_repeat(gif::Repeat::Infinite)
            .map_err(|_| GifError::Encode)?;
        let mut mapper = Mapper::new(&palette, is_exact);
        for f in frames {
            let frame = gif::Frame {
                width,
                height,
                delay: delay_cs,
                buffer: f
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| mapper.index(p))
                    .collect::<Vec<u8>>()
                    .into(),
                ..gif::Frame::default()
            };
            enc.write_frame(&frame).map_err(|_| GifError::Encode)?;
            if enc.get_ref().len() > MAX_BYTES {
                return Err(GifError::TooBig);
            }
        }
    }
    if out.len() > MAX_BYTES {
        return Err(GifError::TooBig);
    }
    Ok(out)
}

/// The JavaScript side: collect frames one at a time, then `finish`.
#[wasm_bindgen(js_name = GifBuilder)]
pub struct GifBuilder {
    width: u16,
    height: u16,
    delay_cs: u16,
    frames: Vec<Vec<u8>>,
    bytes: usize,
}

#[wasm_bindgen(js_class = GifBuilder)]
impl GifBuilder {
    /// `delay_cs`: hundredths of a second per frame (2 or more).
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, delay_cs: u32) -> Result<GifBuilder, JsError> {
        match (u16::try_from(width), u16::try_from(height)) {
            (Ok(w), Ok(h)) if (1..=MAX_EDGE).contains(&w) && (1..=MAX_EDGE).contains(&h) => {
                Ok(GifBuilder {
                    width: w,
                    height: h,
                    delay_cs: delay_cs.clamp(u32::from(MIN_DELAY_CS), u32::from(MAX_DELAY_CS))
                        as u16,
                    frames: Vec::new(),
                    bytes: 0,
                })
            }
            _ => Err(JsError::new(GifError::BadSize.code())),
        }
    }

    /// Add one frame of RGBA bytes. Fails with `GIF_BAD_FRAME`, `GIF_TOO_LONG` (past 15 s) or `GIF_TOO_MUCH`.
    #[wasm_bindgen(js_name = addFrame)]
    pub fn add_frame(&mut self, rgba: &[u8]) -> Result<u32, JsError> {
        let want = usize::from(self.width) * usize::from(self.height) * 4;
        if rgba.len() != want {
            return Err(JsError::new(GifError::BadFrame.code()));
        }
        if (self.frames.len() as u64 + 1) * u64::from(self.delay_cs) * 10
            > u64::from(MAX_DURATION_MS)
        {
            return Err(JsError::new(GifError::TooLong.code()));
        }
        if self.bytes + want > MAX_BUFFERED {
            return Err(JsError::new(GifError::TooMuch.code()));
        }
        self.bytes += want;
        self.frames.push(rgba.to_vec());
        Ok(self.frames.len() as u32)
    }

    /// Frames added so far.
    #[wasm_bindgen(getter, js_name = frameCount)]
    pub fn frame_count(&self) -> u32 {
        self.frames.len() as u32
    }

    /// Frames that fit in 15 s at this delay.
    #[wasm_bindgen(getter, js_name = maxFrames)]
    pub fn max_frames(&self) -> u32 {
        MAX_DURATION_MS / (u32::from(self.delay_cs) * 10)
    }

    /// Write the file. Fails with `GIF_EMPTY` or `GIF_TOO_BIG` (past 20 MiB) among others.
    pub fn finish(&mut self) -> Result<Vec<u8>, JsError> {
        let frames = std::mem::take(&mut self.frames);
        self.bytes = 0;
        encode(self.width, self.height, self.delay_cs, &frames).map_err(|e| JsError::new(e.code()))
    }
}

/// The limits, for the UI: `[max duration ms, max bytes, max edge, min delay cs]`.
#[wasm_bindgen(js_name = gifLimits)]
pub fn gif_limits() -> Vec<u32> {
    vec![
        MAX_DURATION_MS,
        MAX_BYTES as u32,
        u32::from(MAX_EDGE),
        u32::from(MIN_DELAY_CS),
    ]
}

/// Shared by the unit tests and the wasm test.
#[cfg(test)]
pub mod testing {
    /// Decode a GIF into RGBA frames and their delays.
    pub fn decode(bytes: &[u8]) -> Vec<(Vec<u8>, u16)> {
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut dec = opts.read_info(bytes).unwrap();
        let mut out = Vec::new();
        while let Some(f) = dec.read_next_frame().unwrap() {
            out.push((f.buffer.to_vec(), f.delay));
        }
        out
    }

    /// A moving block on a coloured background: few colours, so the palette is exact.
    pub fn sequence(n: usize, w: u16, h: u16) -> Vec<Vec<u8>> {
        (0..n)
            .map(|i| {
                let mut f = Vec::new();
                for y in 0..usize::from(h) {
                    for x in 0..usize::from(w) {
                        let inside = x >= i && x < i + 4 && (2..6).contains(&y);
                        let c = if inside {
                            [255, (i * 20) as u8, 0]
                        } else {
                            [10, 20, 30 + (y % 3) as u8]
                        };
                        f.extend_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                }
                f
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{decode, sequence};
    use super::*;

    #[test]
    fn ten_frames_round_trip_exactly() {
        let frames = sequence(10, 24, 12);
        let bytes = encode(24, 12, 8, &frames).unwrap();
        assert_eq!(&bytes[..6], b"GIF89a");
        let back = decode(&bytes);
        assert_eq!(back.len(), 10);
        for (i, (px, delay)) in back.iter().enumerate() {
            assert_eq!(*delay, 8);
            assert_eq!(px, &frames[i], "frame {i}");
        }
    }

    #[test]
    fn many_colours_quantise_close() {
        // A smooth 2D gradient with thousands of colours, three frames.
        let frames: Vec<Vec<u8>> = (0..3u32)
            .map(|k| {
                (0..64u32 * 64)
                    .flat_map(|i| {
                        let (x, y) = (i % 64, i / 64);
                        [(x * 4) as u8, (y * 4) as u8, (k * 60 + x + y) as u8, 255]
                    })
                    .collect()
            })
            .collect();
        let back = decode(&encode(64, 64, 5, &frames).unwrap());
        assert_eq!(back.len(), 3);
        let mut err = 0u64;
        let mut n = 0u64;
        for (orig, (got, _)) in frames.iter().zip(&back) {
            for (a, b) in orig
                .as_chunks::<4>()
                .0
                .iter()
                .zip(got.as_chunks::<4>().0.iter())
            {
                for c in 0..3 {
                    err += u64::from(a[c].abs_diff(b[c]));
                    n += 1;
                }
            }
        }
        let mean = err as f64 / n as f64;
        assert!(mean < 12.0, "mean channel error {mean}");
    }

    #[test]
    fn duration_cap_is_fifteen_seconds() {
        // 100 frames at 15 cs = 15 s: allowed. 101 frames: too long.
        let one = sequence(1, 4, 4).remove(0);
        assert!(encode(4, 4, 15, &vec![one.clone(); 100]).is_ok());
        assert_eq!(encode(4, 4, 15, &vec![one; 101]), Err(GifError::TooLong));
    }

    #[test]
    fn size_cap_is_twenty_mebibytes() {
        // Noise does not compress: 24 frames of 1024x1024 passes 20 MiB well inside the 15 s cap.
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8
        };
        // Random picks from 256 distinct colours: the palette is exact and the indices do not compress.
        let frames: Vec<Vec<u8>> = (0..24)
            .map(|_| {
                (0..1024 * 1024)
                    .flat_map(|_| {
                        let b = next();
                        [b, 255 - b, 7, 255]
                    })
                    .collect()
            })
            .collect();
        assert_eq!(encode(1024, 1024, 10, &frames), Err(GifError::TooBig));
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(encode(0, 4, 5, &[vec![0; 16]]), Err(GifError::BadSize));
        assert_eq!(encode(4, 4, 5, &[]), Err(GifError::Empty));
        assert_eq!(encode(4, 4, 5, &[vec![0; 15]]), Err(GifError::BadFrame));
    }

    #[test]
    fn short_delays_are_raised_to_the_minimum() {
        let back = decode(&encode(4, 4, 0, &sequence(2, 4, 4)).unwrap());
        assert_eq!(back[0].1, MIN_DELAY_CS);
    }
}
