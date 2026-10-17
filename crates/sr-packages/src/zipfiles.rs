//! Reading a package zip into a map of package-relative paths to bytes. A security boundary: names, types
//! and sizes are checked before anything is kept, and sizes are counted while inflating, never trusted
//! from the headers. Regular files only, `/` separators, no `.`/`..`/hidden/empty segments, no encoded or
//! control characters, no encryption, no archive prefix, no two names that differ only in case.

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read};

use zip::read::{ArchiveOffset, Config as ZipConfig};

/// Largest accepted zip, and the largest total after unpacking.
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;
/// Most entries (files and directories).
pub const MAX_ENTRIES: usize = 400;
const MAX_PATH_LEN: usize = 160;
const MAX_SEGMENT_LEN: usize = 80;

/// Package-relative path to bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// Why `path` is not an acceptable package path.
pub fn path_problem(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("is empty");
    }
    if path.len() > MAX_PATH_LEN {
        return Some("is longer than 160 characters");
    }
    if path.starts_with('/') || path.contains('\\') {
        return Some("is not a relative path with / separators");
    }
    if path.contains('%') || path.chars().any(char::is_control) {
        return Some("contains an encoded or control character");
    }
    for seg in path.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Some("has an empty, . or .. segment");
        }
        if seg.starts_with('.') {
            return Some("has a hidden file or directory");
        }
        if seg.len() > MAX_SEGMENT_LEN {
            return Some("has a segment longer than 80 characters");
        }
        if !seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~' | '@' | '+'))
        {
            return Some("uses characters other than letters, digits and . _ - ~ @ +");
        }
    }
    None
}

fn shown(n: &str) -> String {
    n.chars()
        .take(80)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Unpack and check a zip. CPU-bound: call from a blocking thread. The error names the entry at fault.
pub fn unpack(zip_bytes: &[u8]) -> Result<Files, String> {
    if zip_bytes.is_empty() {
        return Err("The upload is empty.".to_owned());
    }
    if zip_bytes.len() as u64 > MAX_BYTES {
        return Err("The zip is larger than 8 MiB.".to_owned());
    }
    let cfg = ZipConfig {
        archive_offset: ArchiveOffset::Known(0),
    };
    let mut zip = zip::ZipArchive::with_config(cfg, Cursor::new(zip_bytes))
        .map_err(|_| "That is not a zip file.".to_owned())?;
    if zip.len() > MAX_ENTRIES {
        return Err(format!("The zip has more than {MAX_ENTRIES} entries."));
    }
    let mut plan: Vec<(usize, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut declared: u64 = 0;
    for i in 0..zip.len() {
        let f = zip
            .by_index_raw(i)
            .map_err(|_| "The zip index is damaged.".to_owned())?;
        let raw = match f.name() {
            Ok(n) => n.into_owned(),
            Err(_) => return Err("An entry has a name that is not valid.".to_owned()),
        };
        let name = raw.strip_suffix('/').unwrap_or(&raw).to_owned();
        if let Some(p) = path_problem(&name) {
            return Err(format!("{}: {p}", shown(&raw)));
        }
        if f.encrypted() {
            return Err(format!("{}: is encrypted", shown(&raw)));
        }
        if let Some(mode) = f.unix_mode()
            && !matches!(mode & 0o170_000, 0 | 0o100_000 | 0o040_000)
        {
            return Err(format!("{}: is not a regular file", shown(&raw)));
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(format!("{}: is listed twice", shown(&raw)));
        }
        if f.is_dir() {
            continue;
        }
        declared = declared.saturating_add(f.size());
        if declared > MAX_BYTES {
            return Err(format!(
                "{}: makes the package larger than 8 MiB",
                shown(&raw)
            ));
        }
        plan.push((i, name));
    }
    for (_, name) in &plan {
        let mut cut = name.as_str();
        while let Some((parent, _)) = cut.rsplit_once('/') {
            if plan.iter().any(|(_, n)| n == parent) {
                return Err(format!(
                    "{}: sits below a path that is also a file",
                    shown(name)
                ));
            }
            cut = parent;
        }
    }
    let mut files = Files::new();
    let mut total: u64 = 0;
    for (i, name) in plan {
        let mut f = zip
            .by_index(i)
            .map_err(|_| format!("{}: could not be read", shown(&name)))?;
        let room = MAX_BYTES - total;
        let mut data = Vec::new();
        let n = f
            .by_ref()
            .take(room + 1)
            .read_to_end(&mut data)
            .map_err(|_| format!("{}: could not be read", shown(&name)))?;
        if n as u64 > room {
            return Err(format!(
                "{}: makes the package larger than 8 MiB",
                shown(&name)
            ));
        }
        total += n as u64;
        files.insert(name, data);
    }
    Ok(files)
}

/// Build a zip from entries (tests and the CLI's `package pack` helper).
pub fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in entries {
        z.start_file(*n, SimpleFileOptions::default())
            .expect("start entry");
        z.write_all(d).expect("write entry");
    }
    z.finish().expect("finish zip").into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_plain_files() {
        let z = build_zip(&[("a.json", b"{}"), ("assets/x.png", b"x")]);
        let f = unpack(&z).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f["a.json"], b"{}");
    }

    #[test]
    fn refuses_traversal_hidden_and_absolute() {
        for bad in [
            "../a.json",
            "/a.json",
            ".hidden",
            "a//b",
            "a\\b",
            "a%2e.json",
        ] {
            let z = build_zip(&[(bad, b"x")]);
            assert!(unpack(&z).is_err(), "{bad}");
        }
    }

    #[test]
    fn refuses_duplicates_by_case_and_non_zips() {
        let z = build_zip(&[("a.json", b"1"), ("A.json", b"2")]);
        assert!(unpack(&z).unwrap_err().contains("twice"));
        assert!(unpack(b"not a zip").is_err());
        assert!(unpack(b"").is_err());
    }
}
