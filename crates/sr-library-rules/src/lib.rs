//! The pure archive rules of Library ingest (LIB-ING-002, LIB-ING-009).
//!
//! Both the server's hardened archive reader (`sr-library::ingest::archive`) and the browser's
//! pre-checks (`sr-library-wasm`) classify an archive's listing with [`classify`], so a file the
//! browser lets through is judged by the same names, junk, order and size budgets as on the server.
//! No I/O and no dependencies beyond `thiserror`, so it compiles to WebAssembly.

use std::cmp::Ordering;

use thiserror::Error;

/// Metadata files inside an archive are small; anything bigger is not metadata.
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;

/// Page image extensions the ingest decodes (AVIF is refused, D-169).
pub const IMAGE_EXTS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];
pub const AVIF_EXTS: [&str; 2] = ["avif", "avifs"];

/// Budgets for one archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest archive file.
    pub max_archive_bytes: u64,
    /// Most entries in the archive.
    pub max_entries: usize,
    /// Largest uncompressed entry.
    pub max_entry_bytes: u64,
    /// Largest sum of all inflated entries.
    pub max_total_bytes: u64,
    /// Entries at least this big are checked against `max_ratio`.
    pub ratio_floor: u64,
    /// Largest uncompressed : compressed ratio. Page images compress poorly (JPEG ~1,
    /// flat scans well under 100), so 150 only trips on bombs.
    pub max_ratio: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 2 * 1024 * 1024 * 1024,
            max_entries: 4000,
            max_entry_bytes: 64 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            ratio_floor: 1024 * 1024,
            max_ratio: 150,
        }
    }
}

/// Why an archive's listing is refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleError {
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
    /// AVIF pages are refused on purpose (D-169); `files` names them.
    #[error("unsupported format: {what}")]
    UnsupportedFormat {
        what: &'static str,
        files: Vec<String>,
    },
}

/// What every format reports about one entry before anything is inflated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raw {
    pub index: usize,
    pub name: String,
    pub is_dir: bool,
    pub encrypted: bool,
    pub symlink: bool,
    /// Declared uncompressed size.
    pub size: u64,
    /// Compressed size when the format records it per entry.
    pub packed: Option<u64>,
}

/// A page (or metadata file) inside the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub index: usize,
    /// Normalised name: `/` separators, no leading `./`.
    pub name: String,
    /// Declared uncompressed size.
    pub size: u64,
}

/// The classified listing, identical for all formats.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Classified {
    /// Pages in natural order.
    pub pages: Vec<Entry>,
    pub comic_info: Option<Entry>,
    pub series_json: Option<Entry>,
    /// Junk, symlinks, nested archives and other files that are ignored.
    pub skipped: Vec<String>,
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

pub fn is_junk(name: &str) -> bool {
    let lower = name.to_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    lower
        .split('/')
        .any(|c| c == "__macosx" || c == ".git" || c == ".svn")
        || base.starts_with("._")
        || base.starts_with('.')
        || matches!(base, "thumbs.db" | "desktop.ini" | "ehthumbs.db")
}

/// Normalise an entry name and refuse anything that could escape its directory.
pub fn safe_name(raw: &str) -> Result<String, RuleError> {
    let unsafe_name = || RuleError::UnsafePath(raw.chars().take(200).collect());
    if raw.is_empty() || raw.contains('\0') {
        return Err(unsafe_name());
    }
    let name = raw.replace('\\', "/");
    let b = name.as_bytes();
    if name.starts_with('/') || (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()) {
        return Err(unsafe_name());
    }
    let mut parts = Vec::new();
    for c in name.split('/') {
        match c {
            ".." => return Err(unsafe_name()),
            "" | "." => {}
            c => parts.push(c),
        }
    }
    if parts.is_empty() || name.len() > 1024 {
        return Err(unsafe_name());
    }
    Ok(parts.join("/"))
}

pub fn ext(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn base(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// Keep the shallowest candidate; ties by natural order.
fn prefer(slot: &mut Option<Entry>, e: Entry) {
    let depth = |n: &str| n.matches('/').count();
    let better = match slot {
        None => true,
        Some(cur) => (depth(&e.name), &e.name) < (depth(&cur.name), &cur.name),
    };
    if better {
        *slot = Some(e);
    }
}

/// Apply the shared rules to a format's raw listing.
///
/// `solid` formats (7z, RAR) must decode every entry in order to reach a page, so their
/// budget counts all entries; ZIP counts the pages it will actually inflate.
pub fn classify(raws: Vec<Raw>, limits: &Limits, solid: bool) -> Result<Classified, RuleError> {
    let (mut pages, mut comic_info, mut series_json) = (Vec::new(), None, None);
    let (mut skipped, mut avif) = (Vec::new(), Vec::new());
    let mut declared_total = 0u64;
    for r in raws {
        let name = safe_name(&r.name)?;
        if r.encrypted {
            return Err(RuleError::Encrypted(name));
        }
        if r.is_dir || r.name.ends_with('/') {
            continue;
        }
        if r.symlink {
            skipped.push(name);
            continue;
        }
        let size = r.size;
        if size > limits.max_entry_bytes {
            return Err(RuleError::EntryTooLarge(name));
        }
        if let Some(packed) = r.packed
            && size >= limits.ratio_floor
            && size / packed.max(1) > limits.max_ratio
        {
            return Err(RuleError::SuspiciousRatio(name));
        }
        if solid {
            declared_total = declared_total.saturating_add(size);
            if declared_total > limits.max_total_bytes {
                return Err(RuleError::TooLarge("total uncompressed size"));
            }
        }
        if is_junk(&name) {
            skipped.push(name);
            continue;
        }
        let entry = Entry {
            index: r.index,
            name,
            size,
        };
        let file = base(&entry.name).to_lowercase();
        let e = ext(&entry.name);
        if file == "comicinfo.xml" && size <= MAX_METADATA_BYTES {
            prefer(&mut comic_info, entry);
        } else if file == "series.json" && size <= MAX_METADATA_BYTES {
            prefer(&mut series_json, entry);
        } else if AVIF_EXTS.contains(&e.as_str()) {
            avif.push(entry.name);
        } else if IMAGE_EXTS.contains(&e.as_str()) {
            if !solid {
                declared_total = declared_total.saturating_add(size);
                if declared_total > limits.max_total_bytes {
                    return Err(RuleError::TooLarge("total uncompressed size"));
                }
            }
            pages.push(entry);
        } else {
            // Includes nested archives: they are never opened.
            skipped.push(entry.name);
        }
    }
    if !avif.is_empty() {
        avif.sort_by(|a, b| natural_cmp(a, b));
        return Err(RuleError::UnsupportedFormat {
            what: "AVIF pages",
            files: avif,
        });
    }
    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    Ok(Classified {
        pages,
        comic_info,
        series_json,
        skipped,
    })
}

/// The whole-archive ratio, for formats without a per-entry packed size.
pub fn check_archive_ratio(
    declared: u64,
    archive_len: u64,
    limits: &Limits,
) -> Result<(), RuleError> {
    if declared >= limits.ratio_floor && declared / archive_len.max(1) > limits.max_ratio {
        return Err(RuleError::SuspiciousRatio("(whole archive)".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(i: usize, name: &str, size: u64) -> Raw {
        Raw {
            index: i,
            name: name.into(),
            is_dir: false,
            encrypted: false,
            symlink: false,
            size,
            packed: Some(size),
        }
    }

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
        assert_eq!(
            sorted(vec!["B.jpg", "a.jpg", "C.jpg"]),
            ["a.jpg", "B.jpg", "C.jpg"]
        );
        assert_eq!(sorted(vec!["a10", "a"]), ["a", "a10"]);
        let big = "9".repeat(60);
        assert_eq!(natural_cmp(&format!("p{big}"), "p5"), Ordering::Greater);
    }

    #[test]
    fn names_that_escape_are_refused() {
        for bad in [
            "../a.jpg",
            "/etc/p.jpg",
            "C:\\x.jpg",
            "a/../../b.jpg",
            "a\0.jpg",
            "",
        ] {
            assert!(
                matches!(safe_name(bad), Err(RuleError::UnsafePath(_))),
                "{bad:?}"
            );
        }
        assert_eq!(safe_name(".\\ch1\\01.jpg").unwrap(), "ch1/01.jpg");
    }

    #[test]
    fn pages_are_ordered_and_junk_is_skipped() {
        let c = classify(
            vec![
                raw(0, "ch/10.jpg", 5),
                raw(1, "ch/2.PNG", 5),
                raw(2, "__MACOSX/ch/._2.png", 5),
                raw(3, "Thumbs.db", 5),
                raw(4, "ComicInfo.xml", 100),
                raw(5, "notes.txt", 5),
                raw(6, "ch/1.webp", 5),
            ],
            &Limits::default(),
            false,
        )
        .unwrap();
        let names: Vec<&str> = c.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["ch/1.webp", "ch/2.PNG", "ch/10.jpg"]);
        assert!(c.comic_info.is_some());
        assert_eq!(c.skipped, ["__MACOSX/ch/._2.png", "Thumbs.db", "notes.txt"]);
    }

    #[test]
    fn budgets_and_refusals() {
        let l = Limits {
            max_entry_bytes: 100,
            max_total_bytes: 150,
            ..Limits::default()
        };
        assert_eq!(
            classify(vec![raw(0, "a.jpg", 101)], &l, false),
            Err(RuleError::EntryTooLarge("a.jpg".into()))
        );
        assert_eq!(
            classify(vec![raw(0, "a.jpg", 90), raw(1, "b.jpg", 90)], &l, false),
            Err(RuleError::TooLarge("total uncompressed size"))
        );
        // Junk does not count towards a ZIP's budget, but does towards a solid archive's.
        assert!(classify(vec![raw(0, ".hidden", 90), raw(1, "b.jpg", 90)], &l, false).is_ok());
        assert!(classify(vec![raw(0, ".hidden", 90), raw(1, "b.jpg", 90)], &l, true).is_err());
        let mut bomb = raw(0, "a.png", 10 * 1024 * 1024);
        bomb.packed = Some(10);
        assert_eq!(
            classify(vec![bomb], &Limits::default(), false),
            Err(RuleError::SuspiciousRatio("a.png".into()))
        );
        let mut enc = raw(0, "a.jpg", 5);
        enc.encrypted = true;
        assert_eq!(
            classify(vec![enc], &Limits::default(), false),
            Err(RuleError::Encrypted("a.jpg".into()))
        );
        assert!(matches!(
            classify(vec![raw(0, "z.avif", 5), raw(1, "a.avif", 5)], &Limits::default(), false),
            Err(RuleError::UnsupportedFormat { files, .. }) if files == ["a.avif", "z.avif"]
        ));
        assert!(check_archive_ratio(1 << 30, 1 << 20, &Limits::default()).is_err());
        assert!(check_archive_ratio(1 << 30, 1 << 28, &Limits::default()).is_ok());
    }
}
