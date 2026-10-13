//! Library ingest (LIB-ING-*): turn uploaded files into stored, reader-ready pages.
//!
//! * [`image`]: decode a page and produce a WebP original, sized variants,
//!   dimensions, a blurhash and spread detection (LIB-ING-001).
//! * [`archive`]: a hardened ZIP/CBZ, 7z/CB7 and RAR/CBR reader with natural page order and junk
//!   filtering (LIB-ING-002).
//! * [`comicinfo`] and [`series_json`]: metadata files (LIB-ING-003, LIB-ING-006).
//! * [`pipeline`]: archive in, uploaded pages out.
//!
//! Everything here treats its input as hostile: size, entry-count, compression
//! ratio and pixel budgets are enforced before and while decoding, names are never
//! used as file system paths, and metadata files are size-capped.

pub mod archive;
pub mod comicinfo;
pub mod image;
pub mod pipeline;
pub mod series_json;

use sr_storage::img::ImgError;
use std::cmp::Ordering;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestError {
    /// Not a readable archive (ZIP, 7z or RAR).
    #[error("unreadable archive: {0}")]
    Archive(String),
    #[error("archive exceeds the size budget: {0}")]
    TooLarge(&'static str),
    #[error("archive has too many entries")]
    TooManyEntries,
    #[error("entry {0:?} is larger than the per-file limit")]
    EntryTooLarge(String),
    /// A tiny compressed entry that inflates enormously: a decompression bomb.
    #[error("entry {0:?} has a suspicious compression ratio")]
    SuspiciousRatio(String),
    /// Absolute path, `..`, drive letter or NUL in an entry name.
    #[error("entry name {0:?} is not a safe relative path")]
    UnsafePath(String),
    #[error("entry {0:?} is encrypted")]
    Encrypted(String),
    /// A format that is refused on purpose: AVIF pages (D-169) or RAR when the operator
    /// switched it off. `files` names the offending entries, when there are any.
    #[error("unsupported format: {what}{}", list_files(.files))]
    UnsupportedFormat {
        what: &'static str,
        files: Vec<String>,
    },
    #[error("the archive contains no readable pages")]
    NoPages,
    #[error("page {page:?}: {source}")]
    Image {
        page: String,
        #[source]
        source: ImgError,
    },
    #[error("metadata: {0}")]
    Metadata(String),
    #[error("storage: {0}")]
    Storage(String),
}

/// `": a.avif, b.avif"` (at most ten names), or nothing when there are none.
fn list_files(files: &[String]) -> String {
    if files.is_empty() {
        return String::new();
    }
    let shown: Vec<&str> = files.iter().take(10).map(String::as_str).collect();
    let more = files.len().saturating_sub(10);
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    format!(": {}{tail}", shown.join(", "))
}

/// Compare file names the way people read them: case-insensitively, with digit runs
/// ordered by value (`page2` before `page10`). Ties fall back to a plain comparison so
/// the order is total and deterministic.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let run = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut s = String::new();
                    while let Some(&ch) = it.peek() {
                        if !ch.is_ascii_digit() {
                            break;
                        }
                        s.push(ch);
                        it.next();
                    }
                    s
                };
                let (na, nb) = (run(&mut x), run(&mut y));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                // Longer significant run is the bigger number; equal lengths compare as text.
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(c), Some(d)) => {
                let (lc, ld) = (c.to_lowercase().next(), d.to_lowercase().next());
                let ord = lc.cmp(&ld);
                if ord != Ordering::Equal {
                    return ord;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut v: Vec<&str>) -> Vec<&str> {
        v.sort_by(|a, b| natural_cmp(a, b));
        v
    }

    #[test]
    fn digit_runs_sort_by_value() {
        assert_eq!(
            sorted(vec!["p10.jpg", "p2.jpg", "p1.jpg", "p100.jpg", "p20.jpg"]),
            ["p1.jpg", "p2.jpg", "p10.jpg", "p20.jpg", "p100.jpg"]
        );
    }

    #[test]
    fn case_is_ignored_and_ties_are_deterministic() {
        assert_eq!(
            sorted(vec!["B.jpg", "a.jpg", "C.jpg"]),
            ["a.jpg", "B.jpg", "C.jpg"]
        );
        assert_eq!(natural_cmp("a.jpg", "A.jpg"), "a.jpg".cmp("A.jpg"));
        assert_ne!(natural_cmp("a.jpg", "A.jpg"), Ordering::Equal);
        assert_eq!(natural_cmp("x", "x"), Ordering::Equal);
    }

    #[test]
    fn leading_zeros_do_not_change_order_and_paths_group() {
        assert_eq!(
            sorted(vec![
                "ch2/002.png",
                "ch10/001.png",
                "ch2/001.png",
                "ch2/010.png"
            ]),
            ["ch2/001.png", "ch2/002.png", "ch2/010.png", "ch10/001.png"]
        );
        assert_eq!(sorted(vec!["007.jpg", "7.jpg", "8.jpg"])[2], "8.jpg");
    }

    #[test]
    fn prefixes_come_first_and_huge_numbers_do_not_overflow() {
        assert_eq!(sorted(vec!["a10", "a"]), ["a", "a10"]);
        let big = "9".repeat(60);
        assert_eq!(natural_cmp(&format!("p{big}"), "p5"), Ordering::Greater);
    }
}
