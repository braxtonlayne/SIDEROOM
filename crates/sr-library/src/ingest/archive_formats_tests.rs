//! 7z and RAR under the same limits as ZIP, and the cross-format rules. Fixtures are
//! built in the test: 7z with the `sevenz-rust2` writer, RAR5 (stored) by hand, as there
//! is no RAR writer we may use. No binary fixtures are committed.

use super::*;
use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader, encoder_options::AesEncoderOptions};
use std::io::Cursor;

type Reader = ArchiveReader<Cursor<Vec<u8>>>;

fn open(bytes: Vec<u8>, l: ArchiveLimits) -> Result<Reader, IngestError> {
    ArchiveReader::open(Cursor::new(bytes), l)
}

fn names(r: &Reader) -> Vec<&str> {
    r.pages().iter().map(|e| e.name.as_str()).collect()
}

/// Bytes that neither LZMA nor a ratio check will shrink much.
fn noise(n: u32) -> Vec<u8> {
    (0..n)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect()
}

// ---- builders

fn seven_writer() -> ArchiveWriter<Cursor<Vec<u8>>> {
    ArchiveWriter::new(Cursor::new(Vec::new())).unwrap()
}

fn seven(entries: &[(&str, &[u8])], solid: bool) -> Vec<u8> {
    let mut w = seven_writer();
    if solid {
        let es = entries
            .iter()
            .map(|(n, _)| ArchiveEntry::new_file(n))
            .collect();
        let rs = entries
            .iter()
            .map(|(_, d)| SourceReader::new(Cursor::new(d.to_vec())))
            .collect();
        w.push_archive_entries(es, rs).unwrap();
    } else {
        for (n, d) in entries {
            w.push_archive_entry(ArchiveEntry::new_file(n), Some(Cursor::new(d.to_vec())))
                .unwrap();
        }
    }
    w.finish().unwrap().into_inner()
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn vint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// One RAR5 block: CRC32, header size, header, then the data area.
fn rar_block(body: Vec<u8>, data: &[u8]) -> Vec<u8> {
    let mut sized = Vec::new();
    vint(body.len() as u64, &mut sized);
    sized.extend(body);
    let mut out = crc32(&sized).to_le_bytes().to_vec();
    out.extend(sized);
    out.extend_from_slice(data);
    out
}

struct RarFile<'a> {
    name: &'a str,
    data: &'a [u8],
    attr: u64,
    dir: bool,
    /// What the header claims, when it should lie.
    claimed: Option<u64>,
}

fn rf<'a>(name: &'a str, data: &'a [u8]) -> RarFile<'a> {
    RarFile {
        name,
        data,
        attr: 0o100644,
        dir: false,
        claimed: None,
    }
}

const RAR5_SIG: &[u8] = b"Rar!\x1A\x07\x01\x00";

fn rar_main() -> Vec<u8> {
    let mut b = Vec::new();
    vint(1, &mut b); // main header
    vint(0, &mut b); // header flags
    vint(0, &mut b); // archive flags
    rar_block(b, &[])
}

fn rar_end() -> Vec<u8> {
    let mut b = Vec::new();
    vint(5, &mut b);
    vint(0, &mut b);
    vint(0, &mut b);
    rar_block(b, &[])
}

/// A RAR5 archive of stored (method 0) entries.
fn rar(files: &[RarFile]) -> Vec<u8> {
    let mut out = RAR5_SIG.to_vec();
    out.extend(rar_main());
    for f in files {
        let mut b = Vec::new();
        vint(2, &mut b); // file header
        vint(0x2, &mut b); // header flags: data area present
        vint(f.data.len() as u64, &mut b); // packed size
        vint(if f.dir { 0x1 } else { 0x4 }, &mut b); // file flags: directory / CRC32
        vint(f.claimed.unwrap_or(f.data.len() as u64), &mut b);
        vint(f.attr, &mut b);
        if !f.dir {
            b.extend(crc32(f.data).to_le_bytes());
        }
        vint(0, &mut b); // compression: version 0, method store
        vint(1, &mut b); // host OS: Unix
        vint(f.name.len() as u64, &mut b);
        b.extend_from_slice(f.name.as_bytes());
        out.extend(rar_block(b, f.data));
    }
    out.extend(rar_end());
    out
}

// ---- format detection

#[test]
fn formats_are_told_apart_by_their_magic() {
    assert_eq!(
        ArchiveFormat::sniff(b"PK\x03\x04"),
        Some(ArchiveFormat::Zip)
    );
    assert_eq!(
        ArchiveFormat::sniff(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C, 0, 4]),
        Some(ArchiveFormat::SevenZ)
    );
    assert_eq!(ArchiveFormat::sniff(RAR5_SIG), Some(ArchiveFormat::Rar));
    assert_eq!(
        ArchiveFormat::sniff(b"Rar!\x1A\x07\x00"),
        Some(ArchiveFormat::Rar)
    );
    assert_eq!(ArchiveFormat::sniff(b"GIF89a"), None);
    let z = open(seven(&[("1.jpg", b"j")], false), ArchiveLimits::default()).unwrap();
    assert_eq!(z.format(), ArchiveFormat::SevenZ);
    let r = open(rar(&[rf("1.jpg", b"j")]), ArchiveLimits::default()).unwrap();
    assert_eq!(r.format(), ArchiveFormat::Rar);
}

// ---- 7z

#[test]
fn sevenz_pages_metadata_and_junk_behave_like_zip() {
    for solid in [false, true] {
        let a = seven(
            &[
                ("10.jpg", b"ten"),
                ("2.jpg", b"two"),
                ("ComicInfo.xml", b"<ComicInfo/>"),
                ("series.json", b"{}"),
                ("__MACOSX/._2.jpg", b"x"),
                (".DS_Store", b"x"),
                ("readme.txt", b"x"),
                ("nested.zip", b"PK"),
                ("nested.7z", b"x"),
                ("Ch 2/001.png", b"p"),
            ],
            solid,
        );
        let mut r = open(a, ArchiveLimits::default()).unwrap();
        assert_eq!(names(&r), ["2.jpg", "10.jpg", "Ch 2/001.png"], "{solid}");
        assert_eq!(r.comic_info().unwrap().unwrap(), b"<ComicInfo/>");
        assert_eq!(r.series_json().unwrap().unwrap(), b"{}");
        for s in [
            "__MACOSX/._2.jpg",
            ".DS_Store",
            "readme.txt",
            "nested.zip",
            "nested.7z",
        ] {
            assert!(r.skipped().iter().any(|n| n == s), "{s}");
        }
        // Out-of-order reads of a solid block still return the right bytes.
        let pages = r.pages().to_vec();
        assert_eq!(r.read(&pages[2]).unwrap(), b"p");
        assert_eq!(r.read(&pages[0]).unwrap(), b"two");
        assert_eq!(r.read(&pages[1]).unwrap(), b"ten");
    }
}

#[test]
fn sevenz_round_trips_large_pages() {
    let big = noise(300_000);
    let a = seven(&[("1.jpg", &big), ("2.jpg", b"second")], true);
    let mut r = open(a, ArchiveLimits::default()).unwrap();
    let p = r.pages().to_vec();
    assert_eq!(r.read(&p[0]).unwrap(), big);
    assert_eq!(r.read(&p[1]).unwrap(), b"second");
}

#[test]
fn sevenz_traversal_names_are_rejected() {
    for bad in [
        "../evil.jpg",
        "a/../../evil.jpg",
        "..\\evil.jpg",
        "/abs/evil.jpg",
        "C:\\evil.jpg",
    ] {
        let a = seven(&[("ok.jpg", b"j"), (bad, b"x")], false);
        assert!(
            matches!(
                open(a, ArchiveLimits::default()),
                Err(IngestError::UnsafePath(_))
            ),
            "{bad}"
        );
    }
}

#[test]
fn sevenz_bombs_are_rejected_before_decoding() {
    // 12 MiB of zeros packs to a few hundred bytes.
    let zeros = vec![0u8; 12 * 1024 * 1024];
    for solid in [false, true] {
        let a = seven(&[("bomb.jpg", &zeros)], solid);
        assert!(a.len() < 4096, "{}", a.len());
        assert!(matches!(
            open(a, ArchiveLimits::default()),
            Err(IngestError::SuspiciousRatio(_))
        ));
    }
    // Several entries, each fine alone, in one highly compressible solid block.
    let chunk = vec![0u8; 2 * 1024 * 1024];
    let a = seven(
        &[("1.jpg", &chunk), ("2.jpg", &chunk), ("3.jpg", &chunk)],
        true,
    );
    assert!(matches!(
        open(a, ArchiveLimits::default()),
        Err(IngestError::SuspiciousRatio(_))
    ));
}

#[test]
fn sevenz_size_count_and_solid_block_limits() {
    let n = noise(100_000);
    let a = seven(&[("a.jpg", &n), ("b.jpg", &n)], true);
    let entry = ArchiveLimits {
        max_entry_bytes: 50_000,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(a.clone(), entry),
        Err(IngestError::EntryTooLarge(_))
    ));
    let total = ArchiveLimits {
        max_total_bytes: 150_000,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(a.clone(), total),
        Err(IngestError::TooLarge(_))
    ));
    let solid = ArchiveLimits {
        max_solid_block_bytes: 150_000,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(a.clone(), solid),
        Err(IngestError::TooLarge("7z solid block"))
    ));
    let archive = ArchiveLimits {
        max_archive_bytes: 1000,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(a, archive),
        Err(IngestError::TooLarge("archive file"))
    ));
    let many: Vec<(String, &[u8])> = (0..30).map(|i| (format!("{i}.jpg"), &b"x"[..])).collect();
    let refs: Vec<(&str, &[u8])> = many.iter().map(|(n, d)| (n.as_str(), *d)).collect();
    let few = ArchiveLimits {
        max_entries: 10,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(seven(&refs, false), few),
        Err(IngestError::TooManyEntries)
    ));
}

#[test]
fn sevenz_running_budget_stops_repeated_reads() {
    let n = noise(40_000);
    let a = seven(&[("1.jpg", &n), ("2.jpg", &n)], false);
    let l = ArchiveLimits {
        max_total_bytes: 100_000,
        ..ArchiveLimits::default()
    };
    let mut r = open(a, l).unwrap();
    let p = r.pages().to_vec();
    r.read(&p[0]).unwrap();
    r.read(&p[1]).unwrap();
    assert!(matches!(r.read(&p[0]), Err(IngestError::TooLarge(_))));
}

#[test]
fn sevenz_encrypted_archives_are_rejected() {
    for header_too in [true, false] {
        let mut w = seven_writer();
        w.set_content_methods(vec![
            AesEncoderOptions::new(Password::new("secret")).into(),
            sevenz_rust2::EncoderMethod::LZMA2.into(),
        ]);
        w.set_encrypt_header(header_too);
        w.push_archive_entry(
            ArchiveEntry::new_file("1.jpg"),
            Some(Cursor::new(b"j".to_vec())),
        )
        .unwrap();
        let a = w.finish().unwrap().into_inner();
        assert!(
            matches!(
                open(a, ArchiveLimits::default()),
                Err(IngestError::Encrypted(_))
            ),
            "header encrypted: {header_too}"
        );
    }
}

#[test]
fn sevenz_symlinks_and_directories_are_skipped() {
    let mut w = seven_writer();
    w.push_archive_entry(
        ArchiveEntry::new_file("1.jpg"),
        Some(Cursor::new(b"j".to_vec())),
    )
    .unwrap();
    let mut link = ArchiveEntry::new_file("link.jpg");
    link.has_windows_attributes = true;
    link.windows_attributes = 0x8000 | (0o120777 << 16);
    w.push_archive_entry(link, Some(Cursor::new(b"/etc/passwd".to_vec())))
        .unwrap();
    w.push_archive_entry(ArchiveEntry::new_directory("dir"), None::<Cursor<Vec<u8>>>)
        .unwrap();
    let r = open(w.finish().unwrap().into_inner(), ArchiveLimits::default()).unwrap();
    assert_eq!(names(&r), ["1.jpg"]);
    assert!(r.skipped().iter().any(|s| s == "link.jpg"));
}

#[test]
fn sevenz_coder_memory_claims_are_bounded() {
    // LZMA: properties byte, then the dictionary size.
    let mut props = vec![0x5D];
    props.extend((1u32 << 30).to_le_bytes());
    assert_eq!(coder_memory(&SEVENZ_LZMA, &props), 1 << 30);
    assert!(coder_memory(&SEVENZ_LZMA, &props) > MAX_7Z_CODER_MEMORY);
    // LZMA2: one byte, dictionary = (2 | b&1) << (b/2 + 11); 40 means 4 GiB - 1.
    assert_eq!(coder_memory(&SEVENZ_LZMA2, &[24]), 2 << 23);
    assert_eq!(coder_memory(&SEVENZ_LZMA2, &[25]), 3 << 23);
    assert_eq!(coder_memory(&SEVENZ_LZMA2, &[40]), u64::from(u32::MAX));
    assert!(coder_memory(&SEVENZ_LZMA2, &[38]) > MAX_7Z_CODER_MEMORY);
    assert_eq!(coder_memory(&[0x00], &[1, 2, 3, 4, 5]), 0);
    // What the writer emits by default is well inside the limit.
    let a = seven(&[("1.jpg", b"j")], false);
    assert!(open(a, ArchiveLimits::default()).is_ok());
}

#[test]
fn sevenz_garbage_is_not_an_archive() {
    let mut a = seven(&[("1.jpg", b"j")], false);
    let n = a.len();
    a.truncate(n / 2);
    assert!(matches!(
        open(a, ArchiveLimits::default()),
        Err(IngestError::Archive(_))
    ));
    let sig_only = vec![b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C, 0, 4, 1, 2, 3];
    assert!(open(sig_only, ArchiveLimits::default()).is_err());
}

// ---- RAR

#[test]
fn rar_pages_metadata_and_junk_behave_like_zip() {
    let r5 = rar(&[
        rf("10.jpg", b"ten"),
        rf("2.jpg", b"two"),
        rf("ComicInfo.xml", b"<ComicInfo/>"),
        rf("series.json", b"{}"),
        rf("__MACOSX/._2.jpg", b"x"),
        rf(".DS_Store", b"x"),
        rf("readme.txt", b"x"),
        rf("nested.zip", b"PK"),
        rf("nested.rar", b"Rar!"),
        RarFile {
            dir: true,
            ..rf("Ch 2", b"")
        },
        rf("Ch 2/001.png", b"p"),
    ]);
    let mut r = open(r5, ArchiveLimits::default()).unwrap();
    assert_eq!(names(&r), ["2.jpg", "10.jpg", "Ch 2/001.png"]);
    assert_eq!(r.comic_info().unwrap().unwrap(), b"<ComicInfo/>");
    assert_eq!(r.series_json().unwrap().unwrap(), b"{}");
    for s in [
        "__MACOSX/._2.jpg",
        ".DS_Store",
        "readme.txt",
        "nested.zip",
        "nested.rar",
    ] {
        assert!(r.skipped().iter().any(|n| n == s), "{s}");
    }
    // Reading out of archive order reopens the cursor; in order it keeps it.
    let pages = r.pages().to_vec();
    assert_eq!(r.read(&pages[2]).unwrap(), b"p");
    assert_eq!(r.read(&pages[0]).unwrap(), b"two");
    assert_eq!(r.read(&pages[1]).unwrap(), b"ten");
}

#[test]
fn rar_round_trips_large_pages() {
    let big = noise(300_000);
    let mut r = open(
        rar(&[rf("1.jpg", &big), rf("2.jpg", b"second")]),
        ArchiveLimits::default(),
    )
    .unwrap();
    let p = r.pages().to_vec();
    assert_eq!(r.read(&p[0]).unwrap(), big);
    assert_eq!(r.read(&p[1]).unwrap(), b"second");
}

#[test]
fn rar_traversal_names_are_rejected_or_neutralised() {
    // UnRAR itself sanitises some hostile names while it lists them (a leading `..\`
    // is dropped, a drive colon becomes `_`). Whatever it hands us is either refused here
    // or already harmless; nothing with a `..` component or an absolute root gets through.
    for bad in [
        "../evil.jpg",
        "a/../../evil.jpg",
        "..\\evil.jpg",
        "a\\..\\..\\evil.jpg",
        "/abs/evil.jpg",
        "\\abs\\evil.jpg",
        "C:\\evil.jpg",
        "c:/evil.jpg",
    ] {
        let a = rar(&[rf("ok.jpg", b"j"), rf(bad, b"x")]);
        match open(a, ArchiveLimits::default()) {
            Err(IngestError::UnsafePath(_)) => {}
            Err(e) => panic!("{bad}: {e}"),
            Ok(r) => {
                for p in r.pages() {
                    let n = &p.name;
                    assert!(!n.starts_with('/') && !n.contains(':'), "{bad} -> {n}");
                    assert!(!n.split('/').any(|c| c == ".."), "{bad} -> {n}");
                }
            }
        }
    }
    // At least the plain forward-slash forms are refused outright.
    for bad in ["../evil.jpg", "/abs/evil.jpg"] {
        let a = rar(&[rf("ok.jpg", b"j"), rf(bad, b"x")]);
        assert!(
            matches!(
                open(a, ArchiveLimits::default()),
                Err(IngestError::UnsafePath(_))
            ),
            "{bad}"
        );
    }
}

/// RAR headers do not record packed sizes, so a header that claims far more than the file
/// can hold is caught by the whole-archive ratio.
#[test]
fn rar_bombs_are_rejected_before_decoding() {
    let a = rar(&[RarFile {
        claimed: Some(50 * 1024 * 1024),
        ..rf("bomb.jpg", &[0u8; 64])
    }]);
    assert!(a.len() < 1024);
    assert!(matches!(
        open(a, ArchiveLimits::default()),
        Err(IngestError::SuspiciousRatio(_))
    ));
    // Several entries, each under the per-entry cap, over the total budget.
    let many = rar(&[
        RarFile {
            claimed: Some(40 << 20),
            ..rf("1.jpg", b"x")
        },
        RarFile {
            claimed: Some(40 << 20),
            ..rf("2.jpg", b"x")
        },
    ]);
    let l = ArchiveLimits {
        max_total_bytes: 64 << 20,
        max_ratio: u64::MAX,
        ..ArchiveLimits::default()
    };
    assert!(matches!(open(many, l), Err(IngestError::TooLarge(_))));
}

#[test]
fn rar_size_count_and_archive_limits() {
    let n = noise(100_000);
    let a = rar(&[rf("a.jpg", &n), rf("b.jpg", &n)]);
    let entry = ArchiveLimits {
        max_entry_bytes: 50_000,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(a.clone(), entry),
        Err(IngestError::EntryTooLarge(_))
    ));
    let total = ArchiveLimits {
        max_total_bytes: 150_000,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(a.clone(), total),
        Err(IngestError::TooLarge(_))
    ));
    let archive = ArchiveLimits {
        max_archive_bytes: 1000,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(a, archive),
        Err(IngestError::TooLarge("archive file"))
    ));
    let many: Vec<(String, &[u8])> = (0..30).map(|i| (format!("{i}.jpg"), &b"x"[..])).collect();
    let files: Vec<RarFile> = many.iter().map(|(n, d)| rf(n, d)).collect();
    let few = ArchiveLimits {
        max_entries: 10,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        open(rar(&files), few),
        Err(IngestError::TooManyEntries)
    ));
}

#[test]
fn rar_running_budget_stops_repeated_reads() {
    let n = noise(40_000);
    let l = ArchiveLimits {
        max_total_bytes: 100_000,
        ..ArchiveLimits::default()
    };
    let mut r = open(rar(&[rf("1.jpg", &n), rf("2.jpg", &n)]), l).unwrap();
    let p = r.pages().to_vec();
    r.read(&p[0]).unwrap();
    r.read(&p[1]).unwrap();
    assert!(matches!(r.read(&p[0]), Err(IngestError::TooLarge(_))));
}

#[test]
fn rar_encrypted_headers_are_rejected() {
    // An archive encryption header (type 4) right after the main header.
    let mut b = Vec::new();
    vint(4, &mut b);
    vint(0, &mut b); // header flags
    vint(0, &mut b); // encryption version
    vint(0, &mut b); // flags: no password check value
    b.push(15); // KDF count
    b.extend([7u8; 16]); // salt
    let mut a = RAR5_SIG.to_vec();
    a.extend(rar_main());
    a.extend(rar_block(b, &[]));
    a.extend([0u8; 64]);
    assert!(matches!(
        open(a, ArchiveLimits::default()),
        Err(IngestError::Encrypted(_))
    ));
}

#[test]
fn rar_symlinks_are_skipped() {
    let a = rar(&[
        rf("1.jpg", b"j"),
        RarFile {
            attr: 0o120777,
            ..rf("link.jpg", b"/etc/passwd")
        },
    ]);
    let mut r = open(a, ArchiveLimits::default()).unwrap();
    assert_eq!(names(&r), ["1.jpg"]);
    assert!(r.skipped().iter().any(|s| s == "link.jpg"));
    let p = r.pages().to_vec();
    assert_eq!(r.read(&p[0]).unwrap(), b"j");
}

#[test]
fn rar_can_be_switched_off() {
    let a = rar(&[rf("1.jpg", b"j")]);
    let off = ArchiveLimits {
        rar_enabled: false,
        ..ArchiveLimits::default()
    };
    match open(a.clone(), off) {
        Err(IngestError::UnsupportedFormat { what, files }) => {
            assert!(what.contains("RAR"));
            assert!(files.is_empty());
        }
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("opened a RAR with the switch off"),
    }
    // 7z is unaffected.
    assert!(open(seven(&[("1.jpg", b"j")], false), off).is_ok());
    assert!(open(a, ArchiveLimits::default()).is_ok());
}

#[test]
fn rar_garbage_is_not_an_archive() {
    // Cut inside the main header: nothing usable, whether it errors or lists no pages.
    let mut a = rar(&[rf("1.jpg", b"j")]);
    a.truncate(14);
    if let Ok(r) = open(a, ArchiveLimits::default()) {
        assert!(r.pages().is_empty());
    }
    // Cut inside the data of the only page: either refused or the read fails.
    let mut cut = rar(&[rf("1.jpg", &noise(5000))]);
    cut.truncate(cut.len() - 3000);
    if let Ok(mut r) = open(cut, ArchiveLimits::default()) {
        let pages = r.pages().to_vec();
        assert!(pages.is_empty() || r.read(&pages[0]).is_err());
    }
    // A signature followed by junk yields no pages (the pipeline reports `NoPages`).
    if let Ok(r) = open(
        b"Rar!\x1A\x07\x01\x00junkjunkjunk".to_vec(),
        ArchiveLimits::default(),
    ) {
        assert!(r.pages().is_empty());
    }
}

// ---- AVIF (all formats)

#[test]
fn avif_pages_fail_the_archive_with_their_names() {
    use zip::{ZipWriter, write::SimpleFileOptions};
    let names_in = ["1.jpg", "b.avif", "a.AVIF", "10.avif"];
    let zip = {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for n in names_in {
            w.start_file(n, SimpleFileOptions::default()).unwrap();
            w.write_all(b"x").unwrap();
        }
        w.finish().unwrap().into_inner()
    };
    let entries: Vec<(&str, &[u8])> = names_in.iter().map(|n| (*n, &b"x"[..])).collect();
    let files: Vec<RarFile> = names_in.iter().map(|n| rf(n, b"x")).collect();
    for (label, bytes) in [
        ("zip", zip),
        ("7z", seven(&entries, false)),
        ("rar", rar(&files)),
    ] {
        match open(bytes, ArchiveLimits::default()) {
            Err(e) => {
                let IngestError::UnsupportedFormat { what, files } = &e else {
                    panic!("{label}: {e}");
                };
                assert_eq!(*what, "AVIF pages", "{label}");
                assert_eq!(files, &["10.avif", "a.AVIF", "b.avif"], "{label}");
                let msg = e.to_string();
                assert!(msg.contains("a.AVIF") && msg.contains("10.avif"), "{msg}");
            }
            Ok(_) => panic!("{label}: AVIF accepted"),
        }
    }
}

#[test]
fn long_avif_lists_are_shortened_in_the_message() {
    let files: Vec<String> = (0..25).map(|i| format!("{i}.avif")).collect();
    let msg = IngestError::UnsupportedFormat {
        what: "AVIF pages",
        files,
    }
    .to_string();
    assert!(
        msg.contains("0.avif") && msg.contains("and 15 more"),
        "{msg}"
    );
    assert!(!msg.contains("24.avif"));
}
