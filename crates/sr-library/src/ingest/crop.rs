//! Border detection for scanned pages (LIB-ING-007). The result is data only: the stored page
//! keeps its full image and the reader decides whether to crop (the "crop borders" setting).
//!
//! A border is a uniform margin (white paper, black scanner bed). Each side is scanned inward
//! while its rows or columns stay within a small tolerance of the reference colour taken from the
//! page corners. A page whose corners disagree has no border to speak of, so nothing is reported.
//! The detector never removes more than a quarter of any side and ignores margins that are only
//! a few pixels wide.

use ::image::RgbaImage;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Pixels of the full-size page to cut off on each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct PageCrop {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl PageCrop {
    /// Whether the crop leaves a plausible page inside `width` x `height`.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        let w = u64::from(self.left) + u64::from(self.right);
        let h = u64::from(self.top) + u64::from(self.bottom);
        w * 2 < u64::from(width) && h * 2 < u64::from(height)
    }

    pub fn is_empty(&self) -> bool {
        self.left == 0 && self.top == 0 && self.right == 0 && self.bottom == 0
    }
}

/// Largest per-channel difference from the reference colour that still counts as border.
const TOLERANCE: i32 = 14;
/// Share of a row or column that may be off-colour (dust, compression noise) and still be border.
const NOISE_SHARE: f64 = 0.004;
/// Largest share of a side that may be cut.
const MAX_SHARE: f64 = 0.25;
/// Narrowest margin worth reporting, in pixels.
const MIN_MARGIN: u32 = 6;

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    (0..3).all(|i| (i32::from(a[i]) - i32::from(b[i])).abs() <= TOLERANCE)
}

/// The reference colour: the corners must agree, otherwise there is no uniform border.
fn reference(img: &RgbaImage) -> Option<[u8; 4]> {
    let (w, h) = img.dimensions();
    if w < 32 || h < 32 {
        return None;
    }
    let c = [
        img.get_pixel(0, 0).0,
        img.get_pixel(w - 1, 0).0,
        img.get_pixel(0, h - 1).0,
        img.get_pixel(w - 1, h - 1).0,
    ];
    c.iter().all(|p| near(*p, c[0])).then_some(c[0])
}

/// Detect uniform borders. `None` when the page has none worth cropping.
pub fn detect_border(img: &RgbaImage) -> Option<PageCrop> {
    let reference = reference(img)?;
    let (w, h) = img.dimensions();
    let blank_row = |y: u32| {
        let off = (0..w)
            .filter(|&x| !near(img.get_pixel(x, y).0, reference))
            .count();
        (off as f64) <= NOISE_SHARE * f64::from(w)
    };
    let blank_col = |x: u32| {
        let off = (0..h)
            .filter(|&y| !near(img.get_pixel(x, y).0, reference))
            .count();
        (off as f64) <= NOISE_SHARE * f64::from(h)
    };
    let max_v = (f64::from(h) * MAX_SHARE) as u32;
    let max_h = (f64::from(w) * MAX_SHARE) as u32;
    let top = (0..max_v).take_while(|&y| blank_row(y)).count() as u32;
    let bottom = (0..max_v).take_while(|&i| blank_row(h - 1 - i)).count() as u32;
    let left = (0..max_h).take_while(|&x| blank_col(x)).count() as u32;
    let right = (0..max_h).take_while(|&i| blank_col(w - 1 - i)).count() as u32;
    // Keep one pixel of margin so the content does not touch the new edge.
    let trim = |n: u32| if n >= MIN_MARGIN { n - 1 } else { 0 };
    let crop = PageCrop {
        left: trim(left),
        top: trim(top),
        right: trim(right),
        bottom: trim(bottom),
    };
    (!crop.is_empty() && crop.fits(w, h)).then_some(crop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::Rgba;

    /// A `w` x `h` page of `bg` with a `content` coloured block inset by the given margins.
    fn page(
        w: u32,
        h: u32,
        bg: [u8; 4],
        margins: (u32, u32, u32, u32),
        content: [u8; 4],
    ) -> RgbaImage {
        let (l, t, r, b) = margins;
        RgbaImage::from_fn(w, h, |x, y| {
            if x >= l && x < w - r && y >= t && y < h - b {
                Rgba(content)
            } else {
                Rgba(bg)
            }
        })
    }

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const INK: [u8; 4] = [40, 40, 90, 255];

    #[test]
    fn finds_white_margins_on_each_side() {
        let img = page(200, 300, WHITE, (20, 30, 10, 40), INK);
        let crop = detect_border(&img).unwrap();
        // One pixel of margin stays.
        assert_eq!(
            crop,
            PageCrop {
                left: 19,
                top: 29,
                right: 9,
                bottom: 39
            }
        );
    }

    #[test]
    fn finds_dark_scanner_borders() {
        let img = page(200, 300, BLACK, (12, 0, 12, 0), [200, 190, 170, 255]);
        let crop = detect_border(&img).unwrap();
        assert_eq!((crop.left, crop.right), (11, 11));
        assert_eq!((crop.top, crop.bottom), (0, 0));
    }

    #[test]
    fn a_full_bleed_page_has_no_border() {
        // Art to the edge: the corners happen to match, but the edge rows and columns vary.
        let mut img = RgbaImage::from_fn(200, 300, |x, y| {
            Rgba([
                (x * 7 % 256) as u8,
                (y * 5 % 256) as u8,
                ((x + y) * 3 % 256) as u8,
                255,
            ])
        });
        for (x, y) in [(0, 0), (199, 0), (0, 299), (199, 299)] {
            img.put_pixel(x, y, Rgba(INK));
        }
        assert_eq!(detect_border(&img), None);
    }

    #[test]
    fn thin_margins_are_ignored() {
        let img = page(200, 300, WHITE, (3, 3, 3, 3), INK);
        assert_eq!(detect_border(&img), None);
    }

    #[test]
    fn corners_that_disagree_mean_no_border() {
        let mut img = page(200, 300, WHITE, (20, 20, 20, 20), INK);
        img.put_pixel(0, 0, Rgba(BLACK));
        assert_eq!(detect_border(&img), None);
    }

    #[test]
    fn never_cuts_more_than_a_quarter_of_a_side() {
        // A blank page: every row is "border", the scan stops at the cap.
        let img = page(200, 300, WHITE, (0, 0, 0, 0), WHITE);
        let crop = detect_border(&img).unwrap();
        assert!(crop.top <= 75 && crop.bottom <= 75);
        assert!(crop.left <= 50 && crop.right <= 50);
        assert!(crop.fits(200, 300));
    }

    #[test]
    fn tolerates_scan_noise_inside_the_margin() {
        let mut img = page(200, 300, WHITE, (20, 20, 20, 20), INK);
        // Slight paper tint and a speck of dust in the left margin.
        for x in 0..20 {
            for y in 0..300 {
                img.put_pixel(x, y, Rgba([250, 249, 246, 255]));
            }
        }
        img.put_pixel(5, 100, Rgba([90, 90, 90, 255]));
        let crop = detect_border(&img).unwrap();
        assert_eq!(crop.left, 19);
    }

    #[test]
    fn small_images_are_skipped() {
        let img = page(20, 20, WHITE, (4, 4, 4, 4), INK);
        assert_eq!(detect_border(&img), None);
    }

    #[test]
    fn fits_rejects_crops_that_leave_nothing() {
        assert!(
            !PageCrop {
                left: 60,
                top: 0,
                right: 50,
                bottom: 0
            }
            .fits(200, 300)
        );
        assert!(
            PageCrop {
                left: 10,
                top: 0,
                right: 10,
                bottom: 0
            }
            .fits(200, 300)
        );
    }
}
