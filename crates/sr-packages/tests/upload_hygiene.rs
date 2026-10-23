//! RDY-SEC-008: the package zip reader and the sticker, emoji and sound pack validators over generated corpora.
//! Nothing is downloaded: every zip and image here is built from a seeded generator.
//!
//! The limits asserted are the ones in doc 12 section 3 "Upload hygiene"; `the_doc_table_matches_the_code` fails
//! when the table and the constants drift apart.

use std::collections::BTreeSet;

use sr_packages::{
    Checks, PackageKind,
    validate::validate,
    zipfiles::{self, Files, MAX_BYTES, MAX_ENTRIES, build_zip, path_problem, unpack},
};

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

const SEED: u64 = 0x5352_5032_3150_4B47;
const HEAD: &str = r#""id": "my-pack", "name": "My pack", "version": "1.0.0", "author": "me""#;

/// A PNG header of the given size, padded with zeros to `len` bytes (the validator reads the header only).
fn png(w: u32, h: u32, len: usize) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v.resize(len.max(v.len()), 0);
    v
}

fn ogg(len: usize) -> Vec<u8> {
    let mut v = b"OggS\0\x02".to_vec();
    v.resize(len.max(v.len()), 0);
    v
}

fn files(list: Vec<(String, Vec<u8>)>) -> Files {
    list.into_iter().collect()
}

fn pack_json(kind: &str, dir: &str, items: usize, ext: &str) -> String {
    let list: Vec<String> = (0..items)
        .map(|i| {
            format!(
                r#"{{"name": "item_{i:03}", "file": "{dir}/item_{i:03}.{ext}"{}}}"#,
                if kind == "sticker_pack" {
                    r#", "tags": ["wave"]"#
                } else {
                    ""
                }
            )
        })
        .collect();
    format!(
        r#"{{"kind": "{kind}", {HEAD}, "items": [{}]}}"#,
        list.join(",")
    )
}

fn pack(kind: &str, dir: &str, ext: &str, items: usize, media: impl Fn(usize) -> Vec<u8>) -> Files {
    let mut v = vec![(
        "pack.json".to_owned(),
        pack_json(kind, dir, items, ext).into_bytes(),
    )];
    for i in 0..items {
        v.push((format!("{dir}/item_{i:03}.{ext}"), media(i)));
    }
    files(v)
}

fn first_error(f: &Files) -> String {
    validate(f, &Checks::lenient()).unwrap_err().remove(0)
}

// ---- the zip reader

#[test]
fn zip_entry_and_byte_budgets_hold_at_the_limit_and_one_over() {
    assert_eq!((MAX_ENTRIES, MAX_BYTES), (400, 8 * 1024 * 1024));
    let names: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("f{i:04}.txt")).collect();
    let entries = |n: usize| -> Vec<(&str, &[u8])> {
        names[..n].iter().map(|s| (s.as_str(), &b"x"[..])).collect()
    };
    assert_eq!(
        unpack(&build_zip(&entries(MAX_ENTRIES))).unwrap().len(),
        MAX_ENTRIES
    );
    assert!(
        unpack(&build_zip(&entries(MAX_ENTRIES + 1)))
            .unwrap_err()
            .contains("400 entries")
    );

    // One file of exactly the byte budget (a few KiB zipped: zeros) is accepted; one byte more is a bomb.
    let at = vec![0u8; MAX_BYTES as usize];
    assert_eq!(
        unpack(&build_zip(&[("big.bin", &at)])).unwrap()["big.bin"].len(),
        MAX_BYTES as usize
    );
    let over = vec![0u8; MAX_BYTES as usize + 1];
    assert!(
        unpack(&build_zip(&[("big.bin", &over)]))
            .unwrap_err()
            .contains("8 MiB")
    );
    // The budget is for the whole package, not per file.
    let half = vec![0u8; MAX_BYTES as usize / 2];
    let rest = vec![0u8; MAX_BYTES as usize / 2 + 1];
    assert!(unpack(&build_zip(&[("a.bin", &half), ("b.bin", &rest)])).is_err());
    assert!(unpack(&build_zip(&[("a.bin", &half), ("b.bin", &half)])).is_ok());
    // A zip file itself over the limit is refused before it is opened.
    assert!(
        unpack(&vec![0u8; MAX_BYTES as usize + 1])
            .unwrap_err()
            .contains("8 MiB")
    );
}

#[test]
fn every_forbidden_name_segment_is_refused_and_the_length_edges_hold() {
    let long_segment = "a".repeat(81);
    let ok_segment = "a".repeat(80);
    let long_path = format!(
        "{}/{}/{}.png",
        "d".repeat(60),
        "e".repeat(60),
        "f".repeat(35)
    );
    let ok_path = format!(
        "{}/{}/{}.png",
        "d".repeat(60),
        "e".repeat(60),
        "f".repeat(34)
    );
    assert_eq!((long_path.len(), ok_path.len()), (161, 160));
    let forbidden: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("dot segment", "a/./b".into()),
        ("dotdot", "a/../b".into()),
        ("leading dotdot", "../a".into()),
        ("hidden file", ".env".into()),
        ("hidden dir", "ok/.git/config".into()),
        ("absolute", "/etc/passwd".into()),
        ("empty segment", "a//b".into()),
        ("backslash", "a\\b".into()),
        ("drive letter", "C:/x".into()),
        ("drive letter backslash", "C:\\x".into()),
        ("percent", "a%2e%2e/b".into()),
        ("nul", "a\0b".into()),
        ("newline", "a\nb".into()),
        ("del", "a\x7fb".into()),
        ("tab", "a\tb".into()),
        ("space", "a b".into()),
        ("right-to-left override", "gnp.\u{202e}exe".into()),
        ("non-ascii", "caf\u{e9}.png".into()),
        ("colon stream", "a.png:evil".into()),
        ("long segment", long_segment),
        ("long path", long_path),
    ];
    for (what, name) in &forbidden {
        assert!(
            path_problem(name).is_some(),
            "{what}: {name:?} must be refused"
        );
    }
    assert!(path_problem(&ok_segment).is_none());
    assert!(path_problem(&ok_path).is_none());
    // The same names inside a real zip never come out the other side.
    for (what, name) in &forbidden {
        if name.is_empty() || name.contains('\0') {
            continue; // the zip writer will not store these
        }
        let z = build_zip(&[(name.as_str(), b"x")]);
        assert!(unpack(&z).is_err(), "{what}: {name:?} in a zip");
    }
}

#[test]
fn nested_archives_are_data_never_opened_and_an_unlisted_file_fails_the_pack() {
    let inner = build_zip(&[("../../evil.txt", b"x")]);
    let outer = build_zip(&[("inner.zip", &inner), ("note.txt", b"hello")]);
    let f = unpack(&outer).expect("a nested zip is just bytes at this layer");
    assert_eq!(f["inner.zip"], inner);
    assert!(!f.keys().any(|k| k.contains("evil")));
    // A pack that carries one is refused: nothing but the manifest and its listed media may be in it.
    let mut p = pack("sticker_pack", "stickers", "png", 1, |_| png(64, 64, 1000));
    p.insert("extra.zip".to_owned(), outer);
    assert!(first_error(&p).contains("not used"), "{}", first_error(&p));
    // Three levels deep is no different.
    let deep = build_zip(&[(
        "a.zip",
        &build_zip(&[("b.zip", &build_zip(&[("c.txt", b"x")]))]),
    )]);
    assert_eq!(unpack(&deep).unwrap().len(), 1);
}

fn valid_package_zip() -> Vec<u8> {
    let p = pack("sticker_pack", "stickers", "png", 3, |i| {
        png(64, 64, 300 + i)
    });
    let entries: Vec<(&str, &[u8])> = p.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect();
    build_zip(&entries)
}

#[test]
fn every_truncation_of_a_package_zip_is_refused_without_a_panic() {
    let z = valid_package_zip();
    assert!(unpack(&z).is_ok());
    for cut in 0..z.len() {
        assert!(
            unpack(&z[..cut]).is_err(),
            "a zip cut at {cut} of {} cannot be complete",
            z.len()
        );
    }
}

#[test]
fn random_corruption_of_a_package_zip_never_panics_or_escapes_the_budget() {
    let z = valid_package_zip();
    let mut rng = Rng(SEED);
    let mut accepted = 0;
    for round in 0..1500 {
        let mut b = z.clone();
        for _ in 0..=rng.below(6) {
            let at = rng.below(b.len());
            match rng.below(4) {
                0 => b[at] ^= 1 << rng.below(8),
                1 => b[at] = rng.next() as u8,
                2 => b.insert(at, rng.next() as u8),
                _ => {
                    b.remove(at.min(b.len() - 1));
                }
            }
        }
        if let Ok(f) = unpack(&b) {
            accepted += 1;
            let total: usize = f.values().map(Vec::len).sum();
            assert!(
                total as u64 <= MAX_BYTES && f.len() <= MAX_ENTRIES,
                "round {round}"
            );
            for name in f.keys() {
                assert!(
                    path_problem(name).is_none(),
                    "round {round}: {name:?} got through"
                );
            }
            let seen: BTreeSet<String> = f.keys().map(|k| k.to_ascii_lowercase()).collect();
            assert_eq!(
                seen.len(),
                f.len(),
                "round {round}: names that differ only in case"
            );
        }
        let _ = zipfiles::unpack(&b[..b.len() / 2]);
    }
    assert!(
        accepted > 0,
        "the generator should keep some archives intact (padding bytes)"
    );
}

#[test]
fn a_lying_size_in_the_headers_cannot_inflate_past_the_budget() {
    // 64 KiB of zeros, then rewrite the "uncompressed size" of the local and central headers to 4 bytes.
    let data = vec![0u8; 64 * 1024];
    let mut z = build_zip(&[("a.bin", &data)]);
    let sig_local = [0x50, 0x4b, 0x03, 0x04];
    let sig_central = [0x50, 0x4b, 0x01, 0x02];
    let size = (data.len() as u32).to_le_bytes();
    let mut patched = 0;
    for i in 0..z.len().saturating_sub(4) {
        let off = if z[i..i + 4] == sig_local {
            22
        } else if z[i..i + 4] == sig_central {
            24
        } else {
            continue;
        };
        if z[i + off..i + off + 4] == size {
            z[i + off..i + off + 4].copy_from_slice(&4u32.to_le_bytes());
            patched += 1;
        }
    }
    assert!(patched >= 1, "found a size field to patch");
    match unpack(&z) {
        Err(_) => {}
        Ok(f) => assert!(f["a.bin"].len() as u64 <= MAX_BYTES),
    }
}

// ---- pack validators

#[test]
fn sticker_emoji_and_sound_pack_limits_hold_at_the_limit_and_one_over() {
    const KIB: usize = 1024;
    // (kind, dir, ext, item cap, bytes cap, pixel cap)
    let kinds = [
        ("emoji_pack", "emoji", "png", 100usize, 256 * KIB, 512u32),
        ("sticker_pack", "stickers", "png", 40, 512 * KIB, 1024),
    ];
    for (kind, dir, ext, items, bytes, px) in kinds {
        let ok = |n: usize, len: usize, w: u32| pack(kind, dir, ext, n, move |_| png(w, w, len));
        assert!(
            validate(&ok(items, bytes, px), &Checks::lenient()).is_ok(),
            "{kind} at every limit"
        );
        assert!(
            first_error(&ok(items + 1, 100, 16)).contains("items"),
            "{kind}: one item too many"
        );
        assert!(
            first_error(&ok(1, bytes + 1, 16)).contains("larger than"),
            "{kind}: one byte over"
        );
        assert!(
            first_error(&ok(1, 100, px + 1)).contains("pixels"),
            "{kind}: one pixel over"
        );
        assert!(
            first_error(&ok(1, 100, 0)).contains("pixels"),
            "{kind}: zero-sized"
        );
        assert!(
            first_error(&pack(kind, dir, ext, 0, |_| vec![])).contains("items"),
            "{kind}: empty pack"
        );
    }
    // Sounds: Ogg or MP3 magic and the byte cap; the duration is checked when a server adds it (sr-chat).
    let sounds = |n: usize, len: usize| pack("sound_pack", "sounds", "ogg", n, move |_| ogg(len));
    assert!(validate(&sounds(40, 512 * KIB), &Checks::lenient()).is_ok());
    assert!(first_error(&sounds(41, 100)).contains("items"));
    assert!(first_error(&sounds(1, 512 * KIB + 1)).contains("larger than"));
    let not_audio = pack("sound_pack", "sounds", "ogg", 1, |_| {
        b"RIFF....WAVE".to_vec()
    });
    assert!(first_error(&not_audio).contains("Ogg or MP3"));
}

#[test]
fn generated_hostile_media_never_passes_as_the_wrong_kind() {
    let webp_head = {
        let mut v = b"RIFF\x1a\0\0\0WEBPVP8L\x0d\0\0\0".to_vec();
        v.extend_from_slice(&[0x2f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        v
    };
    let corpus: Vec<(&str, Vec<u8>)> = vec![
        ("html", b"<html><script>alert(1)</script>".to_vec()),
        ("svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec()),
        ("zip", build_zip(&[("a.txt", b"x")])),
        ("pdf", b"%PDF-1.7\n".to_vec()),
        ("exe", b"MZ\x90\0\x03".to_vec()),
        ("empty", vec![]),
        ("one byte", vec![0x89]),
        ("png magic only", b"\x89PNG\r\n\x1a\n".to_vec()),
        ("truncated gif", b"GIF89a".to_vec()),
    ];
    for kind in [("sticker_pack", "stickers"), ("emoji_pack", "emoji")] {
        for (what, bytes) in &corpus {
            let p = pack(kind.0, kind.1, "png", 1, |_| bytes.clone());
            assert!(
                validate(&p, &Checks::lenient()).is_err(),
                "{} accepted {what}",
                kind.0
            );
        }
    }
    // Stickers are PNG, APNG or GIF (D-283); the same WebP that an emoji pack takes is refused there.
    let sticker = pack("sticker_pack", "stickers", "webp", 1, |_| webp_head.clone());
    assert!(first_error(&sticker).contains("PNG, APNG or GIF"));
    // Sounds: image bytes and markup are not audio.
    for (what, bytes) in &corpus {
        if bytes.starts_with(b"OggS") {
            continue;
        }
        let p = pack("sound_pack", "sounds", "ogg", 1, |_| bytes.clone());
        assert!(
            validate(&p, &Checks::lenient()).is_err(),
            "sound pack accepted {what}"
        );
    }
}

#[test]
fn a_manifest_of_random_junk_never_panics_and_never_validates() {
    let mut rng = Rng(SEED ^ 0xFEED);
    for _ in 0..500 {
        let n = rng.below(600);
        let junk: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        for manifest in [
            "pack.json",
            "theme.json",
            "layout.json",
            "recipe.json",
            "template.json",
        ] {
            let f = files(vec![(manifest.to_owned(), junk.clone())]);
            assert!(validate(&f, &Checks::lenient()).is_err());
        }
    }
    // Several manifests, or none, are ambiguous.
    let f = files(vec![
        ("pack.json".into(), b"{}".to_vec()),
        ("theme.json".into(), b"{}".to_vec()),
    ]);
    assert!(validate(&f, &Checks::lenient()).is_err());
    assert!(PackageKind::parse("sticker_pack").is_some());
}

// ---- the doc

#[test]
fn the_doc_table_matches_the_code() {
    let doc = include_str!("../../../docs/12-deployment-and-operations.md");
    for row in [
        format!(
            "| Package zip | upload size and unpacked size | {} MiB |",
            MAX_BYTES / (1024 * 1024)
        ),
        format!("| Package zip | entries | {MAX_ENTRIES} |"),
        "| Package zip | path length and segment length | 160 and 80 characters |".to_owned(),
        "| Emoji pack | items, file size, pixels per side | 100, 256 KiB, 512 |".to_owned(),
        "| Sticker pack | items, file size, pixels per side | 40, 512 KiB, 1024 |".to_owned(),
        "| Sound pack | items, file size | 40, 512 KiB |".to_owned(),
    ] {
        assert!(
            doc.contains(&row),
            "docs/12 section 3 lacks the row {row:?}"
        );
    }
}
