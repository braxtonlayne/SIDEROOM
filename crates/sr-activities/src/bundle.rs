//! Activity bundles (J-12, D-247): validation and unpacking of the zip an owner uploads. This is a security
//! boundary: whatever passes here is served from the Activity's own origin after review, so every name,
//! type and size is checked before a byte is stored, and the same path rules guard the proxy's lookups.
//!
//! A bundle is a zip of static files:
//!
//! * at most [`MAX_ENTRIES`] entries and [`MAX_UNPACKED_BYTES`] unpacked (the sizes are counted as the data
//!   is read, never trusted from the headers);
//! * entry names use `/`, letters, digits and `. _ - ~ @ +` only, are relative, have no `.` or `..`
//!   segment, no empty segment, no segment starting with `.`, and no percent-encoding or control characters;
//! * regular files and directories only (symlinks, devices and encrypted entries are refused);
//! * every file has an extension on the allow-list ([`content_type_for`]) and no two names differ only in case;
//! * `index.html` sits at the root.
//!
//! Every refusal names the offending entry.

use std::collections::HashSet;
use std::io::{Cursor, Read};

use sha2::{Digest, Sha256};
use zip::read::{ArchiveOffset, Config as ZipConfig};

/// Most entries (files and directories) in a bundle.
pub const MAX_ENTRIES: usize = 2_000;
/// Most bytes after unpacking, and the largest accepted upload.
pub const MAX_UNPACKED_BYTES: u64 = 32 * 1024 * 1024;
/// Longest entry path.
pub const MAX_PATH_LEN: usize = 256;
/// Longest path segment.
pub const MAX_SEGMENT_LEN: usize = 128;

/// The extension allow-list with each extension's content type.
pub const ALLOWED: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("json", "application/json"),
    ("map", "application/json"),
    ("wasm", "application/wasm"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    ("svg", "image/svg+xml"),
    ("ico", "image/x-icon"),
    ("woff2", "font/woff2"),
    ("mp3", "audio/mpeg"),
    ("ogg", "audio/ogg"),
    ("webm", "video/webm"),
    ("txt", "text/plain; charset=utf-8"),
];

/// Why a bundle was refused, naming the entry when one is at fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleError {
    pub entry: Option<String>,
    pub message: String,
}

impl BundleError {
    fn general(message: impl Into<String>) -> Self {
        Self {
            entry: None,
            message: message.into(),
        }
    }

    fn at(entry: &str, message: impl Into<String>) -> Self {
        Self {
            entry: Some(entry.to_owned()),
            message: message.into(),
        }
    }

    /// The text for the owner: `"<entry>: <message>"` or just the message.
    pub fn text(&self) -> String {
        match &self.entry {
            Some(e) => format!("{}: {}", display_name(e), self.message),
            None => self.message.clone(),
        }
    }
}

/// An entry name made safe to show (control characters and over-long names are cut).
fn display_name(n: &str) -> String {
    n.chars()
        .take(80)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// One unpacked file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFile {
    pub path: String,
    pub content_type: &'static str,
    pub data: Vec<u8>,
}

/// A validated bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpacked {
    /// SHA-256 (lowercase hex) of the zip as uploaded.
    pub sha256: String,
    pub files: Vec<BundleFile>,
    /// Entries in the zip, files and directories.
    pub entries: usize,
    /// Total unpacked bytes.
    pub total: u64,
}

/// The content type of a file name by its extension (case-insensitive), `None` when the extension is not
/// on the allow-list.
pub fn content_type_for(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    ALLOWED.iter().find(|(e, _)| *e == ext).map(|(_, ct)| *ct)
}

/// Why `path` (relative, no trailing slash) is not an acceptable bundle path, or `None`. The proxy applies
/// the same rule to request paths, so a lookup can never leave the bundle's prefix.
pub fn path_problem(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("is empty");
    }
    if path.len() > MAX_PATH_LEN {
        return Some("is longer than 256 characters");
    }
    if path.starts_with('/') {
        return Some("is an absolute path");
    }
    if path.contains('\\') {
        return Some("uses a backslash");
    }
    if path.contains('%') {
        return Some("contains an encoded character");
    }
    if path.chars().any(char::is_control) {
        return Some("contains a control character");
    }
    for seg in path.split('/') {
        if seg.is_empty() {
            return Some("has an empty path segment");
        }
        if seg == ".." || seg == "." {
            return Some("leaves the bundle with . or ..");
        }
        if seg.starts_with('.') {
            return Some("has a hidden file or directory");
        }
        if seg.len() > MAX_SEGMENT_LEN {
            return Some("has a segment longer than 128 characters");
        }
        // A drive letter ("C:") or any other character outside the allow-list.
        if !seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~' | '@' | '+'))
        {
            return Some("uses characters other than letters, digits and . _ - ~ @ +");
        }
    }
    None
}

/// Map a request path (without query) of the Activity origin to the bundle path it asks for: `/` and any
/// path ending in `/` are the `index.html` there. `None` when the path is not an acceptable bundle path.
pub fn request_to_path(request_path: &str) -> Option<String> {
    let rest = request_path.strip_prefix('/')?;
    let rel = if rest.is_empty() {
        "index.html".to_owned()
    } else if let Some(dir) = rest.strip_suffix('/') {
        format!("{dir}/index.html")
    } else {
        rest.to_owned()
    };
    path_problem(&rel).is_none().then_some(rel)
}

/// Validate and unpack a zip. CPU-bound: call from a blocking thread.
pub fn unpack(zip_bytes: &[u8]) -> Result<Unpacked, BundleError> {
    if zip_bytes.is_empty() {
        return Err(BundleError::general("The upload is empty."));
    }
    if zip_bytes.len() as u64 > MAX_UNPACKED_BYTES {
        return Err(BundleError::general(
            "The zip is larger than 32 MiB.".to_owned(),
        ));
    }
    let sha256 = data_encoding::HEXLOWER.encode(Sha256::digest(zip_bytes).as_slice());
    // The archive must start at byte 0: no prefixed (self-extracting) archives.
    let cfg = ZipConfig {
        archive_offset: ArchiveOffset::Known(0),
    };
    let mut zip = zip::ZipArchive::with_config(cfg, Cursor::new(zip_bytes))
        .map_err(|_| BundleError::general("That is not a zip file."))?;
    if zip.len() > MAX_ENTRIES {
        return Err(BundleError::general(format!(
            "The zip has more than {MAX_ENTRIES} entries."
        )));
    }

    // First pass over the metadata: names, types and the declared sizes, before any data is inflated.
    let mut plan: Vec<(usize, String, &'static str)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut declared: u64 = 0;
    for i in 0..zip.len() {
        let f = zip
            .by_index_raw(i)
            .map_err(|_| BundleError::general("The zip index is damaged."))?;
        let raw = match f.name() {
            Ok(n) => n.into_owned(),
            Err(_) => {
                return Err(BundleError::at(
                    &String::from_utf8_lossy(f.name_raw()),
                    "has a name that is not valid",
                ));
            }
        };
        let is_dir = f.is_dir();
        let name = raw.strip_suffix('/').unwrap_or(&raw).to_owned();
        if let Some(p) = path_problem(&name) {
            return Err(BundleError::at(&raw, p));
        }
        if f.encrypted() {
            return Err(BundleError::at(&raw, "is encrypted"));
        }
        if let Some(mode) = f.unix_mode() {
            match mode & 0o170_000 {
                0 | 0o100_000 | 0o040_000 => {}
                0o120_000 => return Err(BundleError::at(&raw, "is a symbolic link")),
                _ => return Err(BundleError::at(&raw, "is not a regular file")),
            }
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(BundleError::at(&raw, "is listed twice"));
        }
        if is_dir {
            continue;
        }
        let Some(ct) = content_type_for(&name) else {
            return Err(BundleError::at(&raw, "has a file type that is not allowed"));
        };
        declared = declared.saturating_add(f.size());
        if declared > MAX_UNPACKED_BYTES {
            return Err(BundleError::at(&raw, "makes the bundle larger than 32 MiB"));
        }
        plan.push((i, name, ct));
    }
    // A file may not also be a directory (`a` and `a/b.js`).
    for (_, name, _) in &plan {
        let mut cut = name.as_str();
        while let Some((parent, _)) = cut.rsplit_once('/') {
            if plan.iter().any(|(_, n, _)| n == parent) {
                return Err(BundleError::at(
                    name,
                    "sits below a path that is also a file",
                ));
            }
            cut = parent;
        }
    }
    if !plan.iter().any(|(_, n, _)| n == "index.html") {
        return Err(BundleError::general(
            "index.html is missing at the root of the bundle.",
        ));
    }

    // Second pass: inflate, counting the real bytes.
    let mut files = Vec::with_capacity(plan.len());
    let mut total: u64 = 0;
    for (i, name, ct) in plan {
        let mut f = zip
            .by_index(i)
            .map_err(|_| BundleError::at(&name, "could not be read"))?;
        let room = MAX_UNPACKED_BYTES - total;
        let mut data = Vec::new();
        let n = f
            .by_ref()
            .take(room + 1)
            .read_to_end(&mut data)
            .map_err(|_| BundleError::at(&name, "could not be read"))?;
        if n as u64 > room {
            return Err(BundleError::at(
                &name,
                "makes the bundle larger than 32 MiB",
            ));
        }
        total += n as u64;
        files.push(BundleFile {
            path: name,
            content_type: ct,
            data,
        });
    }
    Ok(Unpacked {
        sha256,
        entries: seen.len(),
        files,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (n, d) in entries {
            if n.ends_with('/') {
                z.add_directory(*n, SimpleFileOptions::default()).unwrap();
            } else {
                z.start_file(*n, SimpleFileOptions::default()).unwrap();
                z.write_all(d).unwrap();
            }
        }
        z.finish().unwrap().into_inner()
    }

    fn err(entries: &[(&str, &[u8])]) -> BundleError {
        unpack(&build(entries)).unwrap_err()
    }

    #[test]
    fn a_plain_bundle_unpacks_with_content_types() {
        let z = build(&[
            ("index.html", b"<h1>hi</h1>"),
            ("assets/", b""),
            ("assets/app.JS", b"1"),
            ("assets/style.css", b"a{}"),
            ("img/logo.PNG", b"x"),
            ("main.wasm", b"\0asm"),
        ]);
        let u = unpack(&z).unwrap();
        assert_eq!(u.files.len(), 5);
        assert_eq!(u.entries, 6);
        assert_eq!(u.sha256.len(), 64);
        let ct = |p: &str| u.files.iter().find(|f| f.path == p).unwrap().content_type;
        assert_eq!(ct("index.html"), "text/html; charset=utf-8");
        assert_eq!(ct("assets/app.JS"), "text/javascript; charset=utf-8");
        assert_eq!(ct("img/logo.PNG"), "image/png");
        assert_eq!(ct("main.wasm"), "application/wasm");
        assert_eq!(u.total, 11 + 1 + 3 + 1 + 4);
        // The hash is of the zip as uploaded.
        assert_eq!(unpack(&z).unwrap().sha256, u.sha256);
    }

    #[test]
    fn traversal_absolute_and_encoded_names_are_refused_by_name() {
        for bad in [
            "../evil.js",
            "a/../../evil.js",
            "/etc/passwd.txt",
            "a//b.js",
            "a/./b.js",
            "a\\b.js",
            "a%2fb.js",
            "a%2Eb.js",
            "C:/x.js",
            ".hidden.js",
            "dir/.env.txt",
            "sp ace.js",
            "caf\u{e9}.js",
            "nul\0.js",
        ] {
            let e = err(&[("index.html", b"x"), (bad, b"x")]);
            assert!(e.entry.is_some(), "{bad:?} -> {e:?}");
            assert!(e.text().contains(':'), "{bad:?}");
        }
        let long = format!("{}.js", "a".repeat(130));
        assert!(err(&[("index.html", b"x"), (&long, b"x")]).entry.is_some());
        let deep = format!("{}/x.js", "d/".repeat(130).trim_end_matches('/'));
        assert!(err(&[("index.html", b"x"), (&deep, b"x")]).entry.is_some());
    }

    #[test]
    fn extensions_are_allow_listed() {
        for bad in [
            "run.exe",
            "a.php",
            "x.sh",
            "noext",
            "a.html.exe",
            "x.htm",
            "x.xml",
            "x.zip",
        ] {
            let e = err(&[("index.html", b"x"), (bad, b"x")]);
            assert_eq!(e.entry.as_deref(), Some(bad), "{bad}");
            assert!(e.message.contains("not allowed"));
        }
        for ok in [
            "a.mjs", "a.map", "a.jpeg", "a.woff2", "a.mp3", "a.ogg", "a.webm", "a.txt", "a.ico",
            "a.svg", "a.webp", "a.gif", "a.jpg",
        ] {
            unpack(&build(&[("index.html", b"x"), (ok, b"x")])).unwrap();
        }
    }

    #[test]
    fn index_html_must_be_at_the_root() {
        let e = err(&[("sub/index.html", b"x")]);
        assert!(e.entry.is_none() && e.message.contains("index.html"));
        assert!(err(&[]).message.contains("index.html"));
    }

    #[test]
    fn duplicates_ignoring_case_and_file_directory_clashes_are_refused() {
        let e = err(&[("index.html", b"x"), ("A.js", b"1"), ("a.js", b"2")]);
        assert_eq!(e.entry.as_deref(), Some("a.js"));
        let e = err(&[
            ("index.html", b"x"),
            ("lib.js", b"1"),
            ("lib.js/x.js", b"2"),
        ]);
        assert_eq!(e.entry.as_deref(), Some("lib.js/x.js"));
    }

    #[test]
    fn symlinks_and_special_files_are_refused() {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file("index.html", SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"x").unwrap();
        z.add_symlink("link.txt", "/etc/passwd", SimpleFileOptions::default())
            .unwrap();
        let bytes = z.finish().unwrap().into_inner();
        let e = unpack(&bytes).unwrap_err();
        assert_eq!(e.entry.as_deref(), Some("link.txt"));
        assert!(e.message.contains("symbolic link"));
    }

    #[test]
    fn size_and_entry_limits_hold() {
        // 33 files of 1 MiB is more than 32 MiB, however well the zip compresses.
        let big = vec![0u8; 1024 * 1024];
        let mut entries: Vec<(String, &[u8])> = vec![("index.html".into(), b"x")];
        for i in 0..33 {
            entries.push((format!("f{i}.txt"), &big));
        }
        let refs: Vec<(&str, &[u8])> = entries.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        let e = unpack(&build(&refs)).unwrap_err();
        assert!(e.message.contains("32 MiB"), "{e:?}");

        // 2,001 tiny entries.
        let names: Vec<String> = (0..2_000).map(|i| format!("f{i}.txt")).collect();
        let mut v: Vec<(&str, &[u8])> = vec![("index.html", b"x")];
        v.extend(names.iter().map(|n| (n.as_str(), &b"x"[..])));
        assert!(unpack(&build(&v)).unwrap_err().message.contains("2000"));
        // Exactly 2,000 is fine.
        v.pop();
        unpack(&build(&v)).unwrap();
    }

    #[test]
    fn garbage_and_oversize_uploads_are_refused() {
        assert!(unpack(b"").is_err());
        assert!(unpack(b"not a zip").unwrap_err().message.contains("zip"));
        // A zip prefixed by other bytes is not accepted.
        let mut p = vec![0x4d; 40];
        p.extend(build(&[("index.html", b"x")]));
        assert!(unpack(&p).is_err());
        let huge = vec![0u8; (MAX_UNPACKED_BYTES + 1) as usize];
        assert!(unpack(&huge).unwrap_err().message.contains("32 MiB"));
    }

    #[test]
    fn request_paths_map_inside_the_bundle_only() {
        assert_eq!(request_to_path("/").as_deref(), Some("index.html"));
        assert_eq!(request_to_path("/a.js").as_deref(), Some("a.js"));
        assert_eq!(request_to_path("/sub/").as_deref(), Some("sub/index.html"));
        assert_eq!(request_to_path("/x/y.css").as_deref(), Some("x/y.css"));
        for bad in [
            "",
            "x",
            "/..",
            "/a/../b.js",
            "/a/%2e%2e/b.js",
            "//a.js",
            "/a//b.js",
            "/a\\b.js",
            "/.git/config.txt",
            "/a%2Fb.js",
            "/a b.js",
        ] {
            assert_eq!(request_to_path(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn content_types_by_extension() {
        assert_eq!(content_type_for("a/b.SVG"), Some("image/svg+xml"));
        assert_eq!(content_type_for("a/b"), None);
        assert_eq!(content_type_for("a.d/b"), None);
        assert_eq!(content_type_for("x.exe"), None);
    }
}
