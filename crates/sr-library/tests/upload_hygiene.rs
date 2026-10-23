//! RDY-SEC-008: the archive inspector and the page image pipeline over generated corpora. Nothing is downloaded:
//! every archive and image here is built from a seeded generator, so a failure reproduces from the seed.
//!
//! The limits asserted are the ones in doc 12 section 3 "Upload hygiene"; `the_doc_table_matches_the_code` fails
//! when the table and the constants drift apart.

use std::io::{Cursor, Write};

use image::{ImageFormat, Rgba, RgbaImage};
use sevenz_rust2::{ArchiveEntry, ArchiveWriter};
use sr_library::ingest::{
    IngestError,
    archive::{ArchiveLimits, ArchiveReader},
    image::{PageLimits, process_page},
    inspect::inspect,
};
use sr_storage::img::ImgError;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const SEED: u64 = 0x5352_5032_314C_4942;

type Reader = ArchiveReader<Cursor<Vec<u8>>>;

fn open(bytes: Vec<u8>, limits: ArchiveLimits) -> Result<Reader, IngestError> {
    ArchiveReader::open(Cursor::new(bytes), limits)
}

fn zip_with(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    let o = SimpleFileOptions::default().compression_method(method);
    for (name, data) in entries {
        w.start_file(*name, o).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    zip_with(entries, CompressionMethod::Deflated)
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x * 7 % 256) as u8, (y * 5 % 256) as u8, 90, 255])
    });
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

fn noise(n: usize, rng: &mut Rng) -> Vec<u8> {
    (0..n).map(|_| rng.next() as u8).collect()
}

/// Limits small enough to hit in a test; the ratio floor is out of the way unless a test lowers it.
fn tiny() -> ArchiveLimits {
    ArchiveLimits {
        max_archive_bytes: 4 * 1024 * 1024,
        max_entries: 20,
        max_entry_bytes: 1000,
        max_total_bytes: 3000,
        ratio_floor: u64::MAX,
        max_ratio: 150,
        max_solid_block_bytes: 1 << 20,
        rar_enabled: true,
    }
}

fn is<T>(r: &Result<T, IngestError>, want: fn(&IngestError) -> bool) -> bool {
    matches!(r, Err(e) if want(e))
}

// ---- limits at the edge

#[test]
fn entry_count_entry_size_total_size_and_file_size_hold_at_the_limit_and_one_over() {
    let mut rng = Rng(SEED);
    let l = tiny();
    // Entries: 20 pages open, 21 do not.
    let page = noise(10, &mut rng);
    let names: Vec<String> = (0..21).map(|i| format!("p{i:02}.png")).collect();
    let list = |n: usize| -> Vec<(&str, &[u8])> {
        names[..n]
            .iter()
            .map(|s| (s.as_str(), page.as_slice()))
            .collect()
    };
    assert_eq!(open(zip(&list(20)), l).unwrap().pages().len(), 20);
    assert!(is(&open(zip(&list(21)), l), |e| matches!(
        e,
        IngestError::TooManyEntries
    )));

    // One entry: 1000 bytes pass, 1001 do not, and the refusal comes from the header, before inflating.
    let at = noise(1000, &mut rng);
    let over = noise(1001, &mut rng);
    assert!(open(zip(&[("a.png", &at)]), l).is_ok());
    assert!(is(&open(zip(&[("a.png", &over)]), l), |e| matches!(
        e,
        IngestError::EntryTooLarge(_)
    )));

    // The whole archive: three pages of 1000 bytes fill the 3000-byte budget, a fourth byte does not.
    let third = noise(1000, &mut rng);
    let spill = noise(1, &mut rng);
    assert!(
        open(
            zip(&[("a.png", &third), ("b.png", &third), ("c.png", &third)]),
            l
        )
        .is_ok()
    );
    assert!(is(
        &open(
            zip(&[
                ("a.png", &third),
                ("b.png", &third),
                ("c.png", &third),
                ("d.png", &spill)
            ]),
            l
        ),
        |e| matches!(e, IngestError::TooLarge(_))
    ));

    // The archive file itself: exactly the limit opens, one byte less of limit does not.
    let z = zip(&[("a.png", &at)]);
    let mut tight = l;
    tight.max_archive_bytes = z.len() as u64;
    assert!(open(z.clone(), tight).is_ok());
    tight.max_archive_bytes = z.len() as u64 - 1;
    assert!(is(&open(z, tight), |e| matches!(
        e,
        IngestError::TooLarge(_)
    )));
}

#[test]
fn a_compression_bomb_is_refused_from_the_headers_whatever_its_size() {
    let mut l = tiny();
    l.max_entry_bytes = 64 << 20;
    l.max_total_bytes = 1 << 30;
    l.ratio_floor = 64 * 1024;
    // 8 MiB of zeros packs to a few KiB: far past a ratio of 150.
    let zeros = vec![0u8; 8 << 20];
    let bomb = zip(&[("bomb.png", &zeros)]);
    assert!(bomb.len() < 64 * 1024);
    assert!(is(&open(bomb, l), |e| matches!(
        e,
        IngestError::SuspiciousRatio(_)
    )));
    // Many small bombs under the floor do not add up past the total budget either.
    let small = vec![0u8; 60 * 1024];
    let mut many: Vec<(String, &[u8])> = Vec::new();
    for i in 0..30 {
        many.push((format!("b{i:02}.png"), small.as_slice()));
    }
    let entries: Vec<(&str, &[u8])> = many.iter().map(|(n, d)| (n.as_str(), *d)).collect();
    let mut cap = l;
    cap.max_entries = 100;
    cap.max_total_bytes = 20 * 60 * 1024;
    assert!(is(&open(zip(&entries), cap), |e| matches!(
        e,
        IngestError::TooLarge(_)
    )));
    // Incompressible data of the same size is a real page, not a bomb.
    let mut rng = Rng(SEED ^ 1);
    let real = noise(2 << 20, &mut rng);
    assert!(open(zip_with(&[("p.png", &real)], CompressionMethod::Stored), l).is_ok());
}

#[test]
fn nested_archives_and_junk_are_skipped_never_opened() {
    let inner_bomb = zip(&[("../../x.png", b"x")]);
    let mut l = tiny();
    l.max_entry_bytes = 1 << 20;
    l.max_total_bytes = 1 << 20;
    let p = png(8, 8);
    let z = zip(&[
        ("001.png", &p),
        ("inner.zip", &inner_bomb),
        ("inner.cbz", &inner_bomb),
        ("inner.7z", b"7z\xBC\xAF\x27\x1C"),
        ("inner.rar", b"Rar!\x1A\x07\x01\x00"),
        ("__MACOSX/._001.png", b"x"),
        (".DS_Store", b"x"),
        ("Thumbs.db", b"x"),
        ("notes.txt", b"hello"),
    ]);
    let r = open(z, l).unwrap();
    let pages: Vec<&str> = r.pages().iter().map(|e| e.name.as_str()).collect();
    assert_eq!(pages, ["001.png"]);
    for skipped in [
        "inner.zip",
        "inner.cbz",
        "inner.7z",
        "inner.rar",
        "notes.txt",
        ".DS_Store",
    ] {
        assert!(
            r.skipped().iter().any(|s| s == skipped),
            "{skipped} is listed as skipped"
        );
    }
    // Three levels of zips are three skipped files, not three archives.
    let deep = zip(&[
        ("a.zip", &zip(&[("b.zip", &zip(&[("c.png", &p)]))])),
        ("001.png", &p),
    ]);
    let r = open(deep, l).unwrap();
    assert_eq!(r.pages().len(), 1);
    assert!(r.skipped().iter().any(|s| s == "a.zip"));
}

#[test]
fn every_forbidden_name_segment_is_rejected_not_repaired() {
    let p = png(8, 8);
    let mut l = tiny();
    l.max_entry_bytes = 1 << 20;
    l.max_total_bytes = 1 << 20;
    let forbidden = [
        "../x.png",
        "a/../../x.png",
        "..",
        "a/..",
        "..\\x.png",
        "a\\..\\..\\x.png",
        "/abs/x.png",
        "\\abs\\x.png",
        "C:\\x.png",
        "C:/x.png",
        "z:x.png",
        "x\0.png",
        "\0",
        "//server/share/x.png",
    ];
    for name in forbidden {
        let z = zip(&[("001.png", &p), (name, &p)]);
        let r = open(z, l);
        assert!(
            is(&r, |e| matches!(e, IngestError::UnsafePath(_))),
            "{name:?} must be rejected: {:?}",
            r.map(|r| r.pages().len())
        );
    }
    // The long-name edge: 1024 bytes are accepted, 1025 are not.
    let at = format!("{}.png", "n".repeat(1020));
    let over = format!("{}.png", "n".repeat(1021));
    assert_eq!((at.len(), over.len()), (1024, 1025));
    assert!(open(zip(&[(at.as_str(), &p)]), l).is_ok());
    assert!(is(&open(zip(&[(over.as_str(), &p)]), l), |e| matches!(
        e,
        IngestError::UnsafePath(_)
    )));
    // Odd but harmless names are normalised into labels and never become paths.
    let r = open(
        zip(&[
            ("./001.png", &p),
            ("a//b.png", &p),
            ("\u{202e}gnp.exe.png", &p),
            ("sp ace/p 2.png", &p),
        ]),
        l,
    )
    .unwrap();
    let names: Vec<&str> = r.pages().iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"001.png") && names.contains(&"a/b.png"),
        "{names:?}"
    );
    assert!(names.iter().all(|n| !n.starts_with('/')
        && !n.contains("..")
        && !n.contains('\\')
        && !n.contains('\0')));
    // AVIF pages are refused as a set, with their names.
    let r = open(
        zip(&[("001.avif", b"x"), ("002.avif", b"x"), ("003.png", &p)]),
        l,
    );
    assert!(is(
        &r,
        |e| matches!(e, IngestError::UnsupportedFormat { files, .. } if files.len() == 2)
    ));
}

// ---- truncation and mutation

fn comic(rng: &mut Rng) -> Vec<u8> {
    let pages: Vec<Vec<u8>> = (0..4).map(|i| png(8 + i, 8 + i)).collect();
    let xml =
        br#"<?xml version="1.0"?><ComicInfo><Series>S</Series><Number>1</Number></ComicInfo>"#;
    let junk = noise(300, rng);
    zip(&[
        ("c/001.png", &pages[0]),
        ("c/002.png", &pages[1]),
        ("c/010.png", &pages[2]),
        ("c/011.png", &pages[3]),
        ("ComicInfo.xml", xml),
        ("c/notes.bin", &junk),
    ])
}

fn roomy() -> ArchiveLimits {
    ArchiveLimits {
        max_archive_bytes: 1 << 20,
        max_entries: 50,
        max_entry_bytes: 1 << 16,
        max_total_bytes: 1 << 18,
        ratio_floor: 1 << 14,
        max_ratio: 150,
        max_solid_block_bytes: 1 << 20,
        rar_enabled: true,
    }
}

/// Whatever an archive turned out to be, everything read from it stays inside the limits.
fn assert_bounded(what: &str, bytes: Vec<u8>, l: ArchiveLimits) {
    if let Ok(mut r) = open(bytes, l) {
        let pages = r.pages().to_vec();
        assert!(pages.len() <= l.max_entries, "{what}");
        let mut total = 0u64;
        for p in pages {
            assert!(
                !p.name.contains("..") && !p.name.starts_with('/') && !p.name.contains('\0'),
                "{what}: {:?}",
                p.name
            );
            if let Ok(data) = r.read(&p) {
                assert!(data.len() as u64 <= l.max_entry_bytes, "{what}");
                total += data.len() as u64;
            }
        }
        assert!(total <= l.max_total_bytes, "{what}: read {total} bytes");
        let _ = r.comic_info();
        let _ = r.series_json();
    }
}

#[test]
fn every_truncation_of_a_cbz_and_a_7z_is_refused_or_bounded() {
    let mut rng = Rng(SEED ^ 2);
    let l = roomy();
    let cbz = comic(&mut rng);
    assert_eq!(open(cbz.clone(), l).unwrap().pages().len(), 4);
    for cut in 0..cbz.len() {
        assert_bounded(&format!("zip cut at {cut}"), cbz[..cut].to_vec(), l);
        assert!(
            open(cbz[..cut].to_vec(), l).is_err(),
            "a zip cut at {cut} has no end record"
        );
        // inspect() never panics either, and says why it refused.
        let rep = inspect(Cursor::new(cbz[..cut].to_vec()), l);
        assert!(rep.pages == 0 && !rep.problems.is_empty(), "cut at {cut}");
    }
    let p = png(8, 8);
    let mut w = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    for n in ["a.png", "b.png", "c.png"] {
        w.push_archive_entry(ArchiveEntry::new_file(n), Some(Cursor::new(p.clone())))
            .unwrap();
    }
    let sevenz = w.finish().unwrap().into_inner();
    assert_eq!(open(sevenz.clone(), l).unwrap().pages().len(), 3);
    for cut in 0..sevenz.len() {
        assert_bounded(&format!("7z cut at {cut}"), sevenz[..cut].to_vec(), l);
    }
}

fn corrupt(src: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut b = src.to_vec();
    for _ in 0..=rng.below(8) {
        let at = rng.below(b.len().max(1));
        match rng.below(5) {
            0 => {
                if let Some(x) = b.get_mut(at) {
                    *x ^= 1 << rng.below(8);
                }
            }
            1 => {
                for k in at..(at + 1 + rng.below(12)).min(b.len()) {
                    b[k] = rng.next() as u8;
                }
            }
            2 => b.insert(at.min(b.len()), rng.next() as u8),
            3 => {
                if !b.is_empty() {
                    b.remove(at.min(b.len() - 1));
                }
            }
            _ => b.truncate(at),
        }
    }
    b
}

#[test]
fn random_corruption_of_archives_never_panics_or_escapes_the_limits() {
    let mut rng = Rng(SEED ^ 3);
    let l = roomy();
    let cbz = comic(&mut rng);
    for round in 0..1500 {
        assert_bounded(&format!("zip round {round}"), corrupt(&cbz, &mut rng), l);
    }
    let p = png(8, 8);
    let mut w = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    for n in ["a.png", "b.png", "c.png"] {
        w.push_archive_entry(ArchiveEntry::new_file(n), Some(Cursor::new(p.clone())))
            .unwrap();
    }
    let sevenz = w.finish().unwrap().into_inner();
    for round in 0..600 {
        assert_bounded(&format!("7z round {round}"), corrupt(&sevenz, &mut rng), l);
    }
}

#[test]
fn rar_headers_followed_by_noise_are_refused_without_crashing_the_process() {
    let mut rng = Rng(SEED ^ 4);
    let l = roomy();
    for round in 0..150 {
        let mut b = if round % 2 == 0 {
            b"Rar!\x1A\x07\x01\x00".to_vec()
        } else {
            b"Rar!\x1A\x07\x00".to_vec()
        };
        b.extend(noise(rng.below(400), &mut rng));
        assert_bounded(&format!("rar round {round}"), b, l);
    }
    // Switched off by the operator, the same bytes are refused up front.
    let mut off = l;
    off.rar_enabled = false;
    let r = open(b"Rar!\x1A\x07\x01\x00....".to_vec(), off);
    assert!(is(&r, |e| matches!(
        e,
        IngestError::UnsupportedFormat { .. }
    )));
}

#[test]
fn polyglot_archives_are_bounded_like_any_other() {
    let mut rng = Rng(SEED ^ 5);
    let l = roomy();
    let z = comic(&mut rng);
    let p = png(8, 8);
    // A PNG, a GIF and an HTML page in front of a real archive (self-extracting style), and the reverse.
    for (what, prefix) in [
        ("png+zip", p.clone()),
        ("gif+zip", b"GIF89a\x08\x00\x08\x00\x00\x00\x00;".to_vec()),
        (
            "html+zip",
            b"<html><script>alert(1)</script></html>".to_vec(),
        ),
    ] {
        let mut b = prefix.clone();
        b.extend_from_slice(&z);
        assert_bounded(what, b.clone(), l);
        let rep = inspect(Cursor::new(b), l);
        assert!(rep.pages <= 4, "{what}");
        let mut rev = z.clone();
        rev.extend_from_slice(&prefix);
        assert_bounded(&format!("zip+{what}"), rev, l);
    }
    // Something that is not an archive is said to be that, whatever its first bytes claim.
    for junk in [&p[..], b"GIF89a....", b"<svg/>", b"%PDF-1.7", b""] {
        let rep = inspect(Cursor::new(junk.to_vec()), l);
        assert!(rep.pages == 0 && !rep.problems.is_empty());
    }
}

// ---- the page image pipeline

fn limits_for(w: u32, h: u32) -> PageLimits {
    PageLimits {
        max_bytes: 64 << 20,
        max_pixels: u64::from(w) * u64::from(h),
    }
}

#[test]
fn page_pixel_and_byte_budgets_hold_at_the_limit_and_one_over() {
    let d = PageLimits::default();
    assert_eq!((d.max_bytes, d.max_pixels), (64 << 20, 60_000_000));
    let p = png(100, 50);
    assert!(process_page(&p, &limits_for(100, 50)).is_ok());
    assert_eq!(
        process_page(&p, &limits_for(100, 49)).map(|_| ()),
        Err(ImgError::TooLarge)
    );
    let mut by = limits_for(100, 50);
    by.max_bytes = p.len() as u64;
    assert!(process_page(&p, &by).is_ok());
    by.max_bytes = p.len() as u64 - 1;
    assert_eq!(process_page(&p, &by).map(|_| ()), Err(ImgError::TooLarge));
}

#[test]
fn page_images_lose_trailers_and_survive_truncation_and_corruption() {
    let mut rng = Rng(SEED ^ 6);
    let l = limits_for(64, 64);
    let base = png(40, 30);
    for (what, trailer) in [
        ("zip", &b"PK\x03\x04polyglot\x00PK\x05\x06"[..]),
        ("html", &b"<html><script>alert(1)</script>"[..]),
    ] {
        let mut b = base.clone();
        b.extend_from_slice(trailer);
        let out = process_page(&b, &l).unwrap_or_else(|e| panic!("png+{what}: {e:?}"));
        for bytes in std::iter::once(&out.webp).chain(out.variants.iter().map(|v| &v.webp)) {
            assert!(
                bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP",
                "png+{what}: the output is a WebP"
            );
            assert!(
                !bytes.windows(4).any(|w| w == b"PK\x03\x04" || w == b"<htm"),
                "png+{what}"
            );
        }
    }
    for cut in 0..base.len() {
        let _ = process_page(&base[..cut], &l);
    }
    for _ in 0..400 {
        if let Ok(out) = process_page(&corrupt(&base, &mut rng), &l) {
            assert!(u64::from(out.width) * u64::from(out.height) <= l.max_pixels);
        }
    }
}

// ---- the doc

#[test]
fn the_doc_table_matches_the_code() {
    let doc = include_str!("../../../docs/12-deployment-and-operations.md");
    let a = ArchiveLimits::default();
    let p = PageLimits::default();
    const MIB: u64 = 1024 * 1024;
    for row in [
        format!(
            "| Library archive (CBZ, CB7, CBR) | file size | {} GiB |",
            a.max_archive_bytes / (1024 * MIB)
        ),
        format!(
            "| Library archive (CBZ, CB7, CBR) | entries | {} |",
            a.max_entries
        ),
        format!(
            "| Library archive (CBZ, CB7, CBR) | one entry, inflated | {} MiB |",
            a.max_entry_bytes / MIB
        ),
        format!(
            "| Library archive (CBZ, CB7, CBR) | all entries, inflated | {} GiB |",
            a.max_total_bytes / (1024 * MIB)
        ),
        format!(
            "| Library archive (CBZ, CB7, CBR) | compression ratio, for entries of {} MiB or more | {} |",
            a.ratio_floor / MIB,
            a.max_ratio
        ),
        format!(
            "| Library archive (CBZ, CB7, CBR) | one 7z solid block | {} GiB |",
            a.max_solid_block_bytes / (1024 * MIB)
        ),
        format!(
            "| Library page image | source file | {} MiB |",
            p.max_bytes / MIB
        ),
        format!("| Library page image | pixels | {} |", p.max_pixels),
    ] {
        assert!(
            doc.contains(&row),
            "docs/12 section 3 lacks the row {row:?}"
        );
    }
}
