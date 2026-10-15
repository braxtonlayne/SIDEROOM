//! Browser pre-checks for Library uploads (LIB-ING-009).
//!
//! A CBZ or ZIP keeps its table of contents (the *central directory*) at the end of the file, so the
//! browser reads only the tail of the file and this module lists the entries from it, without
//! inflating anything. The listing is then judged by [`sr_library_rules::classify`], the same function
//! the server's archive reader runs: the same name safety, junk filter, page order, entry and total
//! size budgets, compression-ratio check, encrypted entries and AVIF refusal. A file this check passes
//! can still be refused by the server (an undecodable image, say), but a file it refuses would be
//! refused there too.
//!
//! 7z and RAR archives cannot be listed from a tail; for them only the container and the size are
//! checked.
//!
//! The answer is JSON text. The JS side calls [`precheck_zip`] with the last bytes of the file; when
//! the central directory starts before them the answer is `{ "need_tail": N }` and the caller reads
//! `N` bytes from the end and calls again.

use sr_library_rules::{Limits, Raw, RuleError, classify};
use wasm_bindgen::prelude::*;

const EOCD_SIG: u32 = 0x0605_4b50;
const EOCD64_LOC_SIG: u32 = 0x0706_4b50;
const EOCD64_SIG: u32 = 0x0606_4b50;
const CD_SIG: u32 = 0x0201_4b50;
/// The end-of-central-directory record is at most 22 bytes plus a 65,535-byte comment.
pub const EOCD_SEARCH: usize = 22 + 65_535;
/// A first guess for the tail to read: the record, its comment and room for ~1000 entries.
pub const FIRST_TAIL: usize = 128 * 1024;

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn u64le(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// What reading the tail ended in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tail {
    /// Parsed entries, with the offset (from the start of the file) where each local header begins.
    Entries(Vec<ZipEntry>),
    /// The central directory starts before the bytes given: read this many bytes from the end.
    NeedTail(u64),
    /// Not a ZIP, or damaged.
    Invalid(&'static str),
}

/// One central-directory record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub compressed: u64,
    pub size: u64,
    pub local_offset: u64,
    pub symlink: bool,
}

/// List the entries of a ZIP from the last bytes of the file (`tail`), `file_len` being its length.
pub fn read_central_directory(tail: &[u8], file_len: u64) -> Tail {
    let tail_start = file_len.saturating_sub(tail.len() as u64);
    // The end-of-central-directory record: scan backwards for its signature.
    let search_from = tail.len().saturating_sub(EOCD_SEARCH);
    let Some(eocd) = (search_from..=tail.len().saturating_sub(22))
        .rev()
        .find(|&i| u32le(tail, i) == Some(EOCD_SIG))
    else {
        // A short tail may simply not reach it yet.
        return if (tail.len() as u64) < file_len.min(EOCD_SEARCH as u64) {
            Tail::NeedTail(file_len.min(EOCD_SEARCH as u64))
        } else {
            Tail::Invalid("this is not a ZIP archive")
        };
    };
    let (Some(mut count), Some(mut cd_size), Some(mut cd_offset)) = (
        u16le(tail, eocd + 10).map(u64::from),
        u32le(tail, eocd + 12).map(u64::from),
        u32le(tail, eocd + 16).map(u64::from),
    ) else {
        return Tail::Invalid("the archive's directory is cut short");
    };
    // ZIP64: a locator right before the record points at a larger one.
    if (count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_offset == 0xFFFF_FFFF)
        && eocd >= 20
        && u32le(tail, eocd - 20) == Some(EOCD64_LOC_SIG)
    {
        let Some(rec_off) = u64le(tail, eocd - 20 + 8) else {
            return Tail::Invalid("the archive's directory is cut short");
        };
        if rec_off < tail_start {
            return Tail::NeedTail(file_len - rec_off);
        }
        let at = (rec_off - tail_start) as usize;
        if u32le(tail, at) != Some(EOCD64_SIG) {
            return Tail::Invalid("the archive's directory is damaged");
        }
        let (Some(c), Some(s), Some(o)) = (
            u64le(tail, at + 32),
            u64le(tail, at + 40),
            u64le(tail, at + 48),
        ) else {
            return Tail::Invalid("the archive's directory is cut short");
        };
        (count, cd_size, cd_offset) = (c, s, o);
    }
    if cd_offset.saturating_add(cd_size) > file_len {
        return Tail::Invalid("the archive's directory points outside the file");
    }
    if cd_offset < tail_start {
        return Tail::NeedTail(file_len - cd_offset);
    }
    let mut at = (cd_offset - tail_start) as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        if u32le(tail, at) != Some(CD_SIG) {
            return Tail::Invalid("the archive's directory is damaged");
        }
        let (Some(made), Some(flags), Some(method)) = (
            u16le(tail, at + 4),
            u16le(tail, at + 8),
            u16le(tail, at + 10),
        ) else {
            return Tail::Invalid("the archive's directory is cut short");
        };
        let (
            Some(csize),
            Some(usize32),
            Some(nlen),
            Some(xlen),
            Some(clen),
            Some(attr),
            Some(loff),
        ) = (
            u32le(tail, at + 20),
            u32le(tail, at + 24),
            u16le(tail, at + 28),
            u16le(tail, at + 30),
            u16le(tail, at + 32),
            u32le(tail, at + 38),
            u32le(tail, at + 42),
        )
        else {
            return Tail::Invalid("the archive's directory is cut short");
        };
        let (nlen, xlen, clen) = (nlen as usize, xlen as usize, clen as usize);
        let name_at = at + 46;
        let (Some(name_bytes), Some(extra)) = (
            tail.get(name_at..name_at + nlen),
            tail.get(name_at + nlen..name_at + nlen + xlen),
        ) else {
            return Tail::Invalid("the archive's directory is cut short");
        };
        let (mut size, mut compressed, mut local_offset) =
            (u64::from(usize32), u64::from(csize), u64::from(loff));
        // The ZIP64 extra field holds, in this order, only the values that were 0xFFFFFFFF.
        let mut x = 0;
        while x + 4 <= extra.len() {
            let (id, len) = (
                u16le(extra, x).unwrap_or(0),
                u16le(extra, x + 2).unwrap_or(0) as usize,
            );
            if id == 0x0001 {
                let mut p = x + 4;
                let mut take = |want: bool, into: &mut u64| {
                    if want {
                        if let Some(v) = u64le(extra, p) {
                            *into = v;
                        }
                        p += 8;
                    }
                };
                take(usize32 == 0xFFFF_FFFF, &mut size);
                take(csize == 0xFFFF_FFFF, &mut compressed);
                take(loff == 0xFFFF_FFFF, &mut local_offset);
            }
            x += 4 + len;
        }
        // Bit 11 says UTF-8; otherwise the name is CP437, which this treats as lossy UTF-8: the name is
        // only a label and a sort key.
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        // Made by Unix (host 3) with a symlink mode in the high half of the external attributes.
        let symlink = (made >> 8) == 3 && ((attr >> 16) & 0o170_000) == 0o120_000;
        out.push(ZipEntry {
            name,
            method,
            flags,
            compressed,
            size,
            local_offset,
            symlink,
        });
        at = name_at + nlen + xlen + clen;
    }
    Tail::Entries(out)
}

/// The container a file's first bytes announce.
pub fn sniff(head: &[u8]) -> &'static str {
    if head.starts_with(b"PK") {
        "zip"
    } else if head.starts_with(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
        "7z"
    } else if head.starts_with(b"Rar!\x1A\x07") {
        "rar"
    } else {
        "unknown"
    }
}

fn rule_error(e: &RuleError) -> serde_json::Value {
    let (kind, files): (&str, Vec<String>) = match e {
        RuleError::TooLarge(_) => ("too_large", vec![]),
        RuleError::TooManyEntries => ("too_many_entries", vec![]),
        RuleError::EntryTooLarge(n) => ("entry_too_large", vec![n.clone()]),
        RuleError::SuspiciousRatio(n) => ("suspicious_ratio", vec![n.clone()]),
        RuleError::UnsafePath(n) => ("unsafe_path", vec![n.clone()]),
        RuleError::Encrypted(n) => ("encrypted", vec![n.clone()]),
        RuleError::UnsupportedFormat { files, .. } => ("avif", files.clone()),
    };
    serde_json::json!({
        "kind": kind,
        "message": e.to_string(),
        "files": files.into_iter().take(10).collect::<Vec<_>>(),
    })
}

/// The full pre-check as a JSON value (the wasm entry points stringify it).
pub fn precheck(head: &[u8], tail: &[u8], file_len: u64, limits: &Limits) -> serde_json::Value {
    use serde_json::json;
    let container = sniff(head);
    if file_len > limits.max_archive_bytes {
        return json!({ "ok": false, "container": container,
            "error": rule_error(&RuleError::TooLarge("archive file size")) });
    }
    match container {
        "unknown" => {
            return json!({ "ok": false, "container": container, "error": {
                "kind": "not_an_archive", "message": "This is not a ZIP, 7z or RAR archive.", "files": [] } });
        }
        // They cannot be listed from a tail: the server reads them in full.
        "7z" | "rar" => return json!({ "ok": true, "container": container, "listed": false }),
        _ => {}
    }
    let entries = match read_central_directory(tail, file_len) {
        Tail::Entries(e) => e,
        Tail::NeedTail(n) => return json!({ "need_tail": n }),
        Tail::Invalid(why) => {
            return json!({ "ok": false, "container": container, "error": {
                "kind": "unreadable", "message": why, "files": [] } });
        }
    };
    if entries.len() > limits.max_entries {
        return json!({ "ok": false, "container": container,
            "error": rule_error(&RuleError::TooManyEntries) });
    }
    let raws: Vec<Raw> = entries
        .iter()
        .enumerate()
        .map(|(index, e)| Raw {
            index,
            name: e.name.clone(),
            is_dir: e.name.ends_with('/'),
            encrypted: e.flags & 1 != 0,
            symlink: e.symlink,
            size: e.size,
            packed: Some(e.compressed),
        })
        .collect();
    match classify(raws, limits, false) {
        Err(e) => json!({ "ok": false, "container": container, "error": rule_error(&e) }),
        Ok(c) if c.pages.is_empty() => json!({ "ok": false, "container": container, "error": {
            "kind": "no_pages", "message": "The archive contains no readable pages.", "files": [] } }),
        Ok(c) => {
            // Where the first and last pages sit, so the browser can inflate one and try to decode it.
            let probe = |i: usize| {
                let e = &entries[c.pages[i].index];
                json!({ "name": e.name, "local_offset": e.local_offset,
                    "compressed": e.compressed, "size": e.size, "method": e.method })
            };
            json!({
                "ok": true,
                "container": container,
                "listed": true,
                "pages": c.pages.len(),
                "total_bytes": c.pages.iter().map(|p| p.size).sum::<u64>(),
                "largest_bytes": c.pages.iter().map(|p| p.size).max().unwrap_or(0),
                "first_names": c.pages.iter().take(3).map(|p| p.name.clone()).collect::<Vec<_>>(),
                "comic_info": c.comic_info.is_some(),
                "series_json": c.series_json.is_some(),
                "ignored": c.skipped.len(),
                "first": probe(0),
                "last": probe(c.pages.len() - 1),
            })
        }
    }
}

/// Defaults of the server's limits, for a UI that shows them: `[max_archive_bytes, max_entries,
/// max_entry_bytes, max_total_bytes]`.
#[wasm_bindgen(js_name = limitDefaults)]
pub fn limit_defaults() -> Vec<f64> {
    let l = Limits::default();
    vec![
        l.max_archive_bytes as f64,
        l.max_entries as f64,
        l.max_entry_bytes as f64,
        l.max_total_bytes as f64,
    ]
}

/// How many bytes from the end of the file to read first.
#[wasm_bindgen(js_name = firstTail)]
pub fn first_tail() -> u32 {
    FIRST_TAIL as u32
}

/// Check an archive. `head` is the first bytes of the file (a few dozen are enough), `tail` the last
/// bytes (see [`first_tail`]) and `file_len` the file size. `max_archive_bytes` is the operator's upload
/// limit (0 for the server's archive default). Answers JSON text: `{ok, container, ...}` or
/// `{need_tail: N}`.
#[wasm_bindgen(js_name = precheckArchive)]
pub fn precheck_archive(head: &[u8], tail: &[u8], file_len: f64, max_archive_bytes: f64) -> String {
    let mut limits = Limits::default();
    if max_archive_bytes > 0.0 {
        limits.max_archive_bytes = limits.max_archive_bytes.min(max_archive_bytes as u64);
    }
    precheck(head, tail, file_len as u64, &limits).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn build(files: &[(&str, usize)]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, len) in files {
            z.start_file(*name, SimpleFileOptions::default()).unwrap();
            z.write_all(&vec![7u8; *len]).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    fn check(zip: &[u8], limits: &Limits) -> serde_json::Value {
        precheck(&zip[..zip.len().min(64)], zip, zip.len() as u64, limits)
    }

    #[test]
    fn lists_pages_in_natural_order_and_skips_junk() {
        let z = build(&[
            ("c/10.jpg", 30),
            ("c/2.png", 20),
            ("__MACOSX/c/._2.png", 5),
            ("Thumbs.db", 5),
            ("ComicInfo.xml", 40),
            ("c/1.webp", 10),
        ]);
        let r = check(&z, &Limits::default());
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["pages"], 3);
        assert_eq!(
            r["first_names"],
            serde_json::json!(["c/1.webp", "c/2.png", "c/10.jpg"])
        );
        assert_eq!(r["comic_info"], true);
        assert_eq!(r["ignored"], 2);
        assert_eq!(r["total_bytes"], 60);
        assert_eq!(r["largest_bytes"], 30);
        assert_eq!(r["first"]["name"], "c/1.webp");
        assert_eq!(r["last"]["name"], "c/10.jpg");
    }

    #[test]
    fn refuses_what_the_server_refuses() {
        let l = Limits::default();
        let avif = check(&build(&[("a.avif", 5), ("b.jpg", 5)]), &l);
        assert_eq!(
            (avif["ok"].clone(), avif["error"]["kind"].clone()),
            (false.into(), "avif".into())
        );
        assert_eq!(avif["error"]["files"], serde_json::json!(["a.avif"]));
        let none = check(&build(&[("notes.txt", 5)]), &l);
        assert_eq!(none["error"]["kind"], "no_pages");
        let unsafe_name = check(&build(&[("../evil.jpg", 5)]), &l);
        assert_eq!(unsafe_name["error"]["kind"], "unsafe_path");
        let big = check(
            &build(&[("a.jpg", 200)]),
            &Limits {
                max_entry_bytes: 100,
                ..l
            },
        );
        assert_eq!(big["error"]["kind"], "entry_too_large");
        let many = check(
            &build(&[("a.jpg", 1), ("b.jpg", 1), ("c.jpg", 1)]),
            &Limits {
                max_entries: 2,
                ..l
            },
        );
        assert_eq!(many["error"]["kind"], "too_many_entries");
        let too_big_file = check(
            &build(&[("a.jpg", 1)]),
            &Limits {
                max_archive_bytes: 10,
                ..l
            },
        );
        assert_eq!(too_big_file["error"]["kind"], "too_large");
    }

    #[test]
    fn asks_for_more_of_the_tail_when_the_directory_starts_earlier() {
        let files: Vec<(String, usize)> =
            (0..40).map(|i| (format!("page-{i:03}.jpg"), 50)).collect();
        let refs: Vec<(&str, usize)> = files.iter().map(|(n, l)| (n.as_str(), *l)).collect();
        let z = build(&refs);
        // Only the last 100 bytes: the end record is there but most of the directory is not.
        let tail = &z[z.len() - 100..];
        let r = precheck(&z[..64], tail, z.len() as u64, &Limits::default());
        let need = r["need_tail"].as_u64().expect("asks for more");
        assert!(need > 100 && need <= z.len() as u64);
        let r = precheck(
            &z[..64],
            &z[z.len() - need as usize..],
            z.len() as u64,
            &Limits::default(),
        );
        assert_eq!(r["pages"], 40, "{r}");
    }

    #[test]
    fn other_containers_and_garbage() {
        let l = Limits::default();
        let r = precheck(b"7z\xBC\xAF\x27\x1C....", b"", 1000, &l);
        assert_eq!(
            (r["ok"].clone(), r["container"].clone(), r["listed"].clone()),
            (true.into(), "7z".into(), false.into())
        );
        let r = precheck(b"Rar!\x1A\x07\x01\x00", b"", 1000, &l);
        assert_eq!(r["container"], "rar");
        let r = precheck(b"%PDF-1.7", b"%PDF-1.7", 8, &l);
        assert_eq!(r["error"]["kind"], "not_an_archive");
        // A ZIP signature with no end record is damaged, not "needs more".
        let r = precheck(b"PK\x03\x04junkjunkjunk", b"PK\x03\x04junkjunkjunk", 16, &l);
        assert_eq!(r["error"]["kind"], "unreadable", "{r}");
    }

    /// The browser check and the server's archive reader agree on pages, order and refusals.
    #[test]
    fn matches_the_servers_archive_reader() {
        use sr_library::ingest::archive::{ArchiveLimits, ArchiveReader};
        let cases: Vec<Vec<(&str, usize)>> = vec![
            vec![
                ("p/10.jpg", 3),
                ("p/2.jpg", 3),
                ("p/1.jpg", 3),
                ("x.txt", 1),
                (".DS_Store", 1),
            ],
            vec![
                ("ComicInfo.xml", 9),
                ("b.PNG", 2),
                ("a.gif", 2),
                ("sub/c.webp", 2),
            ],
            vec![("one.jpg", 1)],
            vec![("a.avif", 1)],
            vec![("../up.jpg", 1)],
            vec![("readme.md", 1)],
        ];
        for files in cases {
            let z = build(&files);
            let mine = check(&z, &Limits::default());
            let server = ArchiveReader::open(Cursor::new(z.clone()), ArchiveLimits::default());
            match server {
                Ok(r) => {
                    let names: Vec<String> = r.pages().iter().map(|e| e.name.clone()).collect();
                    if names.is_empty() {
                        assert_eq!(mine["error"]["kind"], "no_pages", "{files:?}");
                    } else {
                        assert_eq!(mine["ok"], true, "{files:?} {mine}");
                        let ours: Vec<String> =
                            serde_json::from_value(mine["first_names"].clone()).unwrap();
                        assert_eq!(ours, names[..names.len().min(3)], "{files:?}");
                        assert_eq!(mine["pages"].as_u64().unwrap() as usize, names.len());
                    }
                }
                Err(e) => assert_eq!(mine["ok"], false, "{files:?}: server said {e}"),
            }
        }
    }
}
