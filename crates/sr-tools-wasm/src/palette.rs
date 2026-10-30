//! Palette extraction by k-means (UTL-IMG-001): the dominant colours of an image, in the browser.
//!
//! Pure and deterministic: the seeding is farthest-point from the most common colour, no random numbers, so the
//! same pixels always give the same palette. Pixels are sampled down to at most [`MAX_SAMPLES`] before the
//! iterations, and fully transparent pixels are ignored.

use wasm_bindgen::prelude::*;

/// Upper bound on the pixels the iterations look at (a stride sample of larger images).
pub const MAX_SAMPLES: usize = 40_000;
/// Largest palette asked for.
pub const MAX_K: usize = 16;
const ITERATIONS: usize = 24;

/// One palette entry: its colour and the share of the sampled pixels it stands for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swatch {
    pub rgb: [u8; 3],
    /// 0.0 to 1.0; the shares of a palette sum to 1.0.
    pub share: f64,
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

fn nearest(centres: &[[f64; 3]], p: [f64; 3]) -> usize {
    let mut best = 0;
    let mut best_d = f64::MAX;
    for (i, c) in centres.iter().enumerate() {
        let d = dist2(*c, p);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

/// The `k` dominant colours of `rgba` (4 bytes per pixel), most common first. Fewer come back when the image
/// has fewer distinct colours. Empty for an image with no visible pixels or `k == 0`.
pub fn extract(rgba: &[u8], k: usize) -> Vec<Swatch> {
    let k = k.min(MAX_K);
    let visible: Vec<[f64; 3]> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] >= 16)
        .map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])])
        .collect();
    if k == 0 || visible.is_empty() {
        return Vec::new();
    }
    let stride = visible.len().div_ceil(MAX_SAMPLES);
    let pts: Vec<[f64; 3]> = visible.iter().step_by(stride).copied().collect();

    // Seed: the most common colour (by 4-bit buckets), then repeatedly the point farthest from every centre.
    let mut counts = std::collections::BTreeMap::<(u8, u8, u8), usize>::new();
    for p in &pts {
        *counts
            .entry(((p[0] as u8) >> 4, (p[1] as u8) >> 4, (p[2] as u8) >> 4))
            .or_default() += 1;
    }
    let (top, _) = counts
        .iter()
        .max_by_key(|(key, n)| (**n, std::cmp::Reverse(**key)))
        .map(|(key, n)| (*key, *n))
        .unwrap_or_default();
    let mut centres = vec![[
        f64::from(top.0 * 16 + 8),
        f64::from(top.1 * 16 + 8),
        f64::from(top.2 * 16 + 8),
    ]];
    while centres.len() < k {
        let mut far = None;
        let mut far_d = 0.0;
        for p in &pts {
            let d = centres
                .iter()
                .map(|c| dist2(*c, *p))
                .fold(f64::MAX, f64::min);
            if d > far_d {
                far_d = d;
                far = Some(*p);
            }
        }
        match far {
            Some(p) => centres.push(p),
            None => break, // every point is already a centre: fewer distinct colours than k
        }
    }

    let mut owner = vec![0usize; pts.len()];
    for _ in 0..ITERATIONS {
        let mut sums = vec![[0.0f64; 3]; centres.len()];
        let mut n = vec![0usize; centres.len()];
        let mut moved = false;
        for (i, p) in pts.iter().enumerate() {
            let c = nearest(&centres, *p);
            if owner[i] != c {
                owner[i] = c;
                moved = true;
            }
            for (s, v) in sums[c].iter_mut().zip(p) {
                *s += v;
            }
            n[c] += 1;
        }
        for (c, centre) in centres.iter_mut().enumerate() {
            if n[c] > 0 {
                *centre = [
                    sums[c][0] / n[c] as f64,
                    sums[c][1] / n[c] as f64,
                    sums[c][2] / n[c] as f64,
                ];
            }
        }
        if !moved {
            break;
        }
    }

    let mut sizes = vec![0usize; centres.len()];
    for o in &owner {
        sizes[*o] += 1;
    }
    let total = pts.len() as f64;
    let mut out: Vec<Swatch> = centres
        .iter()
        .zip(&sizes)
        .filter(|(_, n)| **n > 0)
        .map(|(c, n)| Swatch {
            rgb: [c[0].round() as u8, c[1].round() as u8, c[2].round() as u8],
            share: *n as f64 / total,
        })
        .collect();
    out.sort_by(|a, b| b.share.total_cmp(&a.share).then(a.rgb.cmp(&b.rgb)));
    out
}

/// `#rrggbb`.
pub fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// The palette as JSON: `[{"hex":"#aabbcc","r":..,"g":..,"b":..,"share":0.5}, ...]`.
#[wasm_bindgen(js_name = extractPalette)]
pub fn extract_palette(rgba: &[u8], k: u32) -> String {
    let items: Vec<serde_json::Value> = extract(rgba, k as usize)
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "hex": hex(s.rgb), "r": s.rgb[0], "g": s.rgb[1], "b": s.rgb[2],
                "share": (s.share * 10_000.0).round() / 10_000.0,
            })
        })
        .collect();
    serde_json::Value::Array(items).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(n: usize, rgb: [u8; 3]) -> Vec<u8> {
        (0..n).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect()
    }

    #[test]
    fn finds_the_three_blocks_of_a_flag() {
        let mut px = solid(600, [200, 30, 30]);
        px.extend(solid(300, [250, 250, 250]));
        px.extend(solid(100, [20, 40, 160]));
        let p = extract(&px, 3);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].rgb, [200, 30, 30]);
        assert_eq!(p[1].rgb, [250, 250, 250]);
        assert_eq!(p[2].rgb, [20, 40, 160]);
        assert!((p[0].share - 0.6).abs() < 1e-9);
        assert!((p.iter().map(|s| s.share).sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn deterministic_and_bounded() {
        let px: Vec<u8> = (0..5000u32)
            .flat_map(|i| {
                [
                    (i * 7 % 256) as u8,
                    (i * 13 % 256) as u8,
                    (i * 29 % 256) as u8,
                    255,
                ]
            })
            .collect();
        assert_eq!(extract(&px, 5), extract(&px, 5));
        assert_eq!(extract(&px, 99).len(), MAX_K);
    }

    #[test]
    fn fewer_colours_than_asked() {
        let p = extract(&solid(50, [1, 2, 3]), 8);
        assert_eq!(p.len(), 1);
        assert_eq!(hex(p[0].rgb), "#010203");
    }

    #[test]
    fn transparent_pixels_do_not_count() {
        let mut px = [0u8, 255, 0, 0].repeat(100); // invisible green
        px.extend(solid(10, [9, 9, 9]));
        let p = extract(&px, 4);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].rgb, [9, 9, 9]);
        assert!(extract(&[], 4).is_empty());
        assert!(extract(&px, 0).is_empty());
    }

    #[test]
    fn json_shape() {
        let j: serde_json::Value =
            serde_json::from_str(&extract_palette(&solid(4, [255, 0, 128]), 2)).unwrap();
        assert_eq!(j[0]["hex"], "#ff0080");
        assert_eq!(j[0]["share"], 1.0);
    }
}
