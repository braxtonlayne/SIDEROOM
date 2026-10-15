//! The Library naming fixture suite (REV-UP-007): at least 300 names with the parse they must
//! produce. Hand-written cases follow the spec and common scanlation habits; generated cases
//! cross every notation with series names that contain digits and punctuation.

use super::*;

struct Want {
    role: Role,
    series: Option<&'static str>,
    volume: Option<f64>,
    chapter: Option<f64>,
    title: Option<&'static str>,
    group: Option<&'static str>,
    page: Option<u32>,
}

fn w(role: Role) -> Want {
    Want {
        role,
        series: None,
        volume: None,
        chapter: None,
        title: None,
        group: None,
        page: None,
    }
}

fn check(path: &str, want: &Want) {
    let r = parse_path(path);
    assert_eq!(r.role, want.role, "{path}: {r:?}");
    assert_eq!(r.series.as_deref(), want.series, "{path}: {r:?}");
    assert_eq!(r.volume, want.volume, "{path}: {r:?}");
    assert_eq!(r.chapter, want.chapter, "{path}: {r:?}");
    assert_eq!(r.title.as_deref(), want.title, "{path}: {r:?}");
    assert_eq!(r.group.as_deref(), want.group, "{path}: {r:?}");
    assert_eq!(r.page, want.page, "{path}: {r:?}");
}

fn archive(
    series: &'static str,
    volume: Option<f64>,
    chapter: Option<f64>,
    title: Option<&'static str>,
    group: Option<&'static str>,
) -> Want {
    Want {
        series: Some(series),
        volume,
        chapter,
        title,
        group,
        ..w(Role::Archive)
    }
}

fn page(series: &'static str, chapter: f64, page: u32) -> Want {
    Want {
        series: Some(series),
        chapter: Some(chapter),
        page: Some(page),
        ..w(Role::Page)
    }
}

fn handwritten() -> Vec<(&'static str, Want)> {
    vec![
        // 2.2 archives
        (
            "One Piece v01 c001 - Romance Dawn [Group].cbz",
            archive(
                "One Piece",
                Some(1.0),
                Some(1.0),
                Some("Romance Dawn"),
                Some("Group"),
            ),
        ),
        (
            "One Piece v100 c1000.cbz",
            archive("One Piece", Some(100.0), Some(1000.0), None, None),
        ),
        (
            "One Piece c1000.cbz",
            archive("One Piece", None, Some(1000.0), None, None),
        ),
        (
            "One Piece - c1000.cbz",
            archive("One Piece", None, Some(1000.0), None, None),
        ),
        (
            "one_piece_v02_c010.cbz",
            archive("one piece", Some(2.0), Some(10.0), None, None),
        ),
        (
            "One Piece Vol.03 Ch.021.cbz",
            archive("One Piece", Some(3.0), Some(21.0), None, None),
        ),
        (
            "One Piece Volume 4 Chapter 31.cbz",
            archive("One Piece", Some(4.0), Some(31.0), None, None),
        ),
        (
            "One Piece Chapter 12.5.cbz",
            archive("One Piece", None, Some(12.5), None, None),
        ),
        (
            "One Piece c012.5 [Extra].cbz",
            archive("One Piece", None, Some(12.5), None, Some("Extra")),
        ),
        (
            "One Piece - 012.cbz",
            archive("One Piece", None, Some(12.0), None, None),
        ),
        (
            "One Piece - 012 - The Title.cbz",
            archive("One Piece", None, Some(12.0), Some("The Title"), None),
        ),
        (
            "One Piece c012 (2020) [Digital] [Grp].cbz",
            archive("One Piece", None, Some(12.0), None, Some("Grp")),
        ),
        (
            "Berserk v41 c364 [Dark Horse].zip",
            archive("Berserk", Some(41.0), Some(364.0), None, Some("Dark Horse")),
        ),
        (
            "Berserk v41.cbr",
            archive("Berserk", Some(41.0), None, None, None),
        ),
        (
            "Berserk c001.rar",
            archive("Berserk", None, Some(1.0), None, None),
        ),
        (
            "Berserk c002.cb7",
            archive("Berserk", None, Some(2.0), None, None),
        ),
        (
            "Berserk c003.7z",
            archive("Berserk", None, Some(3.0), None, None),
        ),
        (
            "Vinland Saga v01 c001.cbz",
            archive("Vinland Saga", Some(1.0), Some(1.0), None, None),
        ),
        (
            "Dr. Stone v01 c001.cbz",
            archive("Dr. Stone", Some(1.0), Some(1.0), None, None),
        ),
        (
            "20th Century Boys v01 c001.cbz",
            archive("20th Century Boys", Some(1.0), Some(1.0), None, None),
        ),
        (
            "Mob Psycho 100 c05.cbz",
            archive("Mob Psycho 100", None, Some(5.0), None, None),
        ),
        (
            "Series/Series v01 c001.cbz",
            archive("Series", Some(1.0), Some(1.0), None, None),
        ),
        (
            "Series/Vol 02/Series c010.cbz",
            archive("Series", Some(2.0), Some(10.0), None, None),
        ),
        (
            "Series/Vol 02/c011.cbz",
            archive("Series", Some(2.0), Some(11.0), None, None),
        ),
        (
            "Series/Chapter 5.cbz",
            archive("Series", None, Some(5.0), None, None),
        ),
        (
            "Some Novel.epub",
            Want {
                series: Some("Some Novel"),
                ..w(Role::Document)
            },
        ),
        (
            "Some Novel v02.epub",
            Want {
                series: Some("Some Novel"),
                volume: Some(2.0),
                ..w(Role::Document)
            },
        ),
        (
            "Some Novel c03 - Part Three.pdf",
            Want {
                series: Some("Some Novel"),
                chapter: Some(3.0),
                title: Some("Part Three"),
                ..w(Role::Document)
            },
        ),
        // 2.1 folders of images
        ("Series/Chapter 001/page_001.jpg", page("Series", 1.0, 1)),
        ("Series/Chapter 001/page_002.png", page("Series", 1.0, 2)),
        ("Series/Chapter 001/page_003.webp", page("Series", 1.0, 3)),
        ("Series/Chapter 001/page_004.gif", page("Series", 1.0, 4)),
        ("Series/Chapter 001/page_005.jpeg", page("Series", 1.0, 5)),
        ("Series/Chapter 001/001.jpg", page("Series", 1.0, 1)),
        ("Series/Chapter 001/12.jpg", page("Series", 1.0, 12)),
        ("Series/Chapter 001/p012.jpg", page("Series", 1.0, 12)),
        ("Series/Chapter 001/img_0013.png", page("Series", 1.0, 13)),
        ("Series/Ch.12/page_001.jpg", page("Series", 12.0, 1)),
        ("Series/c12/page_001.jpg", page("Series", 12.0, 1)),
        ("Series/Chapter 12.5/page_001.jpg", page("Series", 12.5, 1)),
        (
            "Series/Chapter 003 - A Title/page_001.jpg",
            Want {
                title: Some("A Title"),
                ..page("Series", 3.0, 1)
            },
        ),
        (
            "Series/Vol 02/Chapter 013 - Title/page_001.jpg",
            Want {
                volume: Some(2.0),
                title: Some("Title"),
                ..page("Series", 13.0, 1)
            },
        ),
        (
            "Series/Vol 02/Chapter 013/page_010.jpg",
            Want {
                volume: Some(2.0),
                ..page("Series", 13.0, 10)
            },
        ),
        (
            "Series/Volume 2/Ch.14/001.png",
            Want {
                volume: Some(2.0),
                ..page("Series", 14.0, 1)
            },
        ),
        ("Series\\Chapter 004\\page_001.jpg", page("Series", 4.0, 1)),
        ("Series/Chapter 001/PAGE_001.JPG", page("Series", 1.0, 1)),
        (
            "Series [Group]/Chapter 001/page_001.jpg",
            Want {
                group: Some("Group"),
                ..page("Series", 1.0, 1)
            },
        ),
        (
            "Series/Chapter 001/page_001_spread.jpg",
            Want {
                ..page("Series", 1.0, 1)
            },
        ),
        // roles
        (
            "Series/series.json",
            Want {
                series: Some("Series"),
                ..w(Role::SeriesJson)
            },
        ),
        ("series.json", w(Role::SeriesJson)),
        (
            "Series/Series c001.cbz",
            archive("Series", None, Some(1.0), None, None),
        ),
        (
            "Series/ComicInfo.xml",
            Want {
                series: Some("Series"),
                ..w(Role::ComicInfo)
            },
        ),
        (
            "Series/Chapter 002/ComicInfo.xml",
            Want {
                series: Some("Series"),
                chapter: Some(2.0),
                ..w(Role::ComicInfo)
            },
        ),
        // refused or unknown
        (
            "Series/Chapter 001/page_001.avif",
            Want {
                series: Some("Series"),
                chapter: Some(1.0),
                ..w(Role::Unknown)
            },
        ),
        (
            "Series/notes.txt",
            Want {
                series: Some("Series"),
                ..w(Role::Unknown)
            },
        ),
        ("", w(Role::Unknown)),
    ]
}

const SERIES: &[&str] = &[
    "One Piece",
    "Berserk",
    "Vinland Saga",
    "Dr. Stone",
    "20th Century Boys",
    "Mob Psycho 100",
    "Monster",
    "Akira",
    "Blame!",
    "Dorohedoro",
    "Gantz",
    "Hellsing",
    "Nana",
    "Bleach",
    "Naruto",
    "Fairy Tail",
    "Slam Dunk",
    "Planetes",
    "Pluto",
    "Uzumaki",
    "Yotsuba&!",
    "Spy x Family",
    "Chainsaw Man",
    "Dandadan",
    "Kaiji",
    "Gon",
    "Lone Wolf and Cub",
    "Saga of Tanya the Evil",
    "Tokyo Ghoul re",
    "The Promised Neverland",
];

fn generated() -> usize {
    let mut n = 0;
    for (i, s) in SERIES.iter().enumerate() {
        let v = (i % 9 + 1) as f64;
        let c = (i * 3 + 1) as f64;
        let vv = format!("{:02}", i % 9 + 1);
        let cc = format!("{:03}", i * 3 + 1);
        let under = s.replace(' ', "_");
        for (p, want_v, title) in [
            (format!("{s} v{vv} c{cc}.cbz"), Some(v), None),
            (format!("{s} v{vv} c{cc} [Grp].cbz"), Some(v), None),
            (
                format!("{s} c{cc} - Some Title.cbz"),
                None,
                Some("Some Title"),
            ),
            (format!("{s} Vol.{vv} Ch.{cc}.zip"), Some(v), None),
            (
                format!("{s}/Vol {vv}/Chapter {cc}/page_001.jpg"),
                Some(v),
                None,
            ),
            (format!("{s}/Chapter {cc}/001.webp"), None, None),
            (format!("{s}/Ch.{cc}/page_12.png"), None, None),
            (format!("{under}_c{cc}.cbr"), None, None),
            (format!("{s} - {cc}.cbz"), None, None),
            (
                format!("{s}/Chapter {cc} - A Title/page_007.jpg"),
                None,
                Some("A Title"),
            ),
        ] {
            let r = parse_path(&p);
            let want_series = if p.contains('_') && !p.contains(' ') {
                under.replace('_', " ")
            } else {
                (*s).to_owned()
            };
            assert_eq!(
                r.series.as_deref(),
                Some(want_series.as_str()),
                "{p}: {r:?}"
            );
            assert_eq!(r.chapter, Some(c), "{p}: {r:?}");
            assert_eq!(r.volume, want_v, "{p}: {r:?}");
            assert_eq!(r.title.as_deref(), title, "{p}: {r:?}");
            n += 1;
        }
    }
    n
}

#[test]
fn handwritten_names_parse_as_documented() {
    let cases = handwritten();
    assert!(cases.len() >= 50, "{}", cases.len());
    let failed = cases
        .iter()
        .filter(|(p, want)| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(p, want))).is_err()
        })
        .count();
    assert_eq!(failed, 0, "{failed} fixtures failed, see above");
}

#[test]
fn spreads_and_avif_are_flagged() {
    let r = parse_path("S/Chapter 1/page_004_spread.jpg");
    assert!(r.spread);
    assert_eq!(r.page, Some(4));
    let r = parse_path("S/Chapter 1/cover.jpg");
    assert_eq!(r.page, None);
    assert!(!r.warnings.is_empty());
    let r = parse_path("S/Chapter 1/page_001.avif");
    assert!(r.warnings.iter().any(|w| w.contains("AVIF")));
}

#[test]
fn the_suite_holds_at_least_300_names() {
    let total = handwritten().len() + generated();
    assert!(total >= 300, "only {total} fixtures");
}

#[test]
fn hostile_input_never_panics() {
    for p in [
        "",
        "/",
        "..",
        "../../x.cbz",
        ".cbz",
        "[[[[]]]].cbz",
        "c99999999999999999999999.cbz",
        "v.cbz",
        "c.cbz",
        "- - -.cbz",
        "\u{0}.jpg",
    ] {
        let _ = parse_path(p);
    }
    let _ = parse_path(&("a".repeat(100_000) + ".cbz"));
    let _ = parse_path(&"[x]".repeat(50_000));
}
