//! The naming fixture suite (REV-UP-007): at least 300 real-world style names with the
//! parse they must produce. Hand-written cases cover the spec's accepted forms and the
//! awkward real-world ones; generated cases cross every episode notation with titles that
//! contain digits, hyphens and punctuation.

use super::*;

enum Case {
    Movie(String, &'static str, Option<u16>, Option<&'static str>),
    Ep(String, &'static str, u32, Vec<u32>, Option<&'static str>),
    Abs(String, &'static str, Vec<u32>),
    Sub(String, Option<&'static str>, bool, bool),
    Art(String, ArtworkKind),
    Unknown(String),
}

fn movie(p: &str, title: &'static str, year: Option<u16>) -> Case {
    Case::Movie(p.into(), title, year, None)
}
fn movie_v(p: &str, title: &'static str, year: u16, variant: &'static str) -> Case {
    Case::Movie(p.into(), title, Some(year), Some(variant))
}
fn ep(p: &str, title: &'static str, season: u32, eps: &[u32], ept: Option<&'static str>) -> Case {
    Case::Ep(p.into(), title, season, eps.to_vec(), ept)
}
fn abs(p: &str, title: &'static str, eps: &[u32]) -> Case {
    Case::Abs(p.into(), title, eps.to_vec())
}
fn sub(p: &str, lang: Option<&'static str>, forced: bool, sdh: bool) -> Case {
    Case::Sub(p.into(), lang, forced, sdh)
}
fn art(p: &str, k: ArtworkKind) -> Case {
    Case::Art(p.into(), k)
}

fn check(c: &Case) {
    match c {
        Case::Movie(p, title, year, variant) => {
            let r = parse_path(p);
            assert_eq!(r.role, Role::Video, "{p}: {r:?}");
            assert_eq!(r.media, Media::Movie, "{p}: {r:?}");
            assert_eq!(r.title, *title, "{p}: {r:?}");
            assert_eq!(r.year, *year, "{p}: {r:?}");
            assert_eq!(r.variant.as_deref(), *variant, "{p}: {r:?}");
        }
        Case::Ep(p, title, season, eps, ept) => {
            let r = parse_path(p);
            assert_eq!(r.media, Media::Episode, "{p}: {r:?}");
            assert_eq!(r.title, *title, "{p}: {r:?}");
            assert_eq!(r.season, Some(*season), "{p}: {r:?}");
            assert_eq!(&r.episodes, eps, "{p}: {r:?}");
            assert_eq!(r.episode_title.as_deref(), *ept, "{p}: {r:?}");
            assert!(!r.absolute, "{p}: {r:?}");
        }
        Case::Abs(p, title, eps) => {
            let r = parse_path(p);
            assert_eq!(r.media, Media::Episode, "{p}: {r:?}");
            assert_eq!(r.title, *title, "{p}: {r:?}");
            assert_eq!(&r.episodes, eps, "{p}: {r:?}");
            assert!(r.absolute, "{p}: {r:?}");
            assert_eq!(r.season, Some(1), "{p}: {r:?}");
        }
        Case::Sub(p, lang, forced, sdh) => {
            let r = parse_path(p);
            assert_eq!(r.role, Role::Subtitle, "{p}: {r:?}");
            assert_eq!(r.language.as_deref(), *lang, "{p}: {r:?}");
            assert_eq!(r.forced, *forced, "{p}: {r:?}");
            assert_eq!(r.sdh, *sdh, "{p}: {r:?}");
        }
        Case::Art(p, k) => {
            let r = parse_path(p);
            assert_eq!(r.role, Role::Artwork, "{p}: {r:?}");
            assert_eq!(r.artwork, Some(*k), "{p}: {r:?}");
        }
        Case::Unknown(p) => {
            let r = parse_path(p);
            assert!(
                r.role == Role::Unknown || r.media == Media::Unknown,
                "{p}: {r:?}"
            );
            assert!(!r.warnings.is_empty(), "{p}: {r:?}");
        }
    }
}

fn handwritten() -> Vec<Case> {
    use ArtworkKind::*;
    vec![
        // 1.1 movies
        movie("Akira (1988)/Akira (1988).mkv", "Akira", Some(1988)),
        movie("Akira (1988).mkv", "Akira", Some(1988)),
        movie("akira (1988).mp4", "akira", Some(1988)),
        movie_v(
            "Akira (1988) - Director's Cut.mkv",
            "Akira",
            1988,
            "Director's Cut",
        ),
        movie_v(
            "Akira (1988)/Akira (1988) - Theatrical.mkv",
            "Akira",
            1988,
            "Theatrical",
        ),
        movie_v(
            "Blade Runner (1982) - Final Cut.mkv",
            "Blade Runner",
            1982,
            "Final Cut",
        ),
        movie_v(
            "Blade Runner (1982) - The Final Cut [1080p].mkv",
            "Blade Runner",
            1982,
            "The Final Cut",
        ),
        movie(
            "Spirited Away (2001) [1080p] [BluRay].mkv",
            "Spirited Away",
            Some(2001),
        ),
        movie(
            "Spirited.Away.2001.1080p.BluRay.x264-GROUP.mkv",
            "Spirited Away",
            Some(2001),
        ),
        movie("Spirited_Away_(2001).mkv", "Spirited Away", Some(2001)),
        movie(
            "Princess Mononoke (1997) (1080p).mkv",
            "Princess Mononoke",
            Some(1997),
        ),
        movie(
            "Perfect Blue (1997) [Dual Audio].mkv",
            "Perfect Blue",
            Some(1997),
        ),
        movie("Your Name (2016).mkv", "Your Name", Some(2016)),
        movie(
            "Your.Name.2016.720p.WEB-DL.AAC.mkv",
            "Your Name",
            Some(2016),
        ),
        movie(
            "The Matrix (1999)/The Matrix (1999).avi",
            "The Matrix",
            Some(1999),
        ),
        movie(
            "2001 A Space Odyssey (1968).mkv",
            "2001 A Space Odyssey",
            Some(1968),
        ),
        movie(
            "2001 - A Space Odyssey (1968).mkv",
            "2001 - A Space Odyssey",
            Some(1968),
        ),
        movie("Se7en (1995).m4v", "Se7en", Some(1995)),
        movie(
            "Blade Runner 2049 (2017).mkv",
            "Blade Runner 2049",
            Some(2017),
        ),
        movie(
            "Mad Max Fury Road (2015).webm",
            "Mad Max Fury Road",
            Some(2015),
        ),
        movie("Movies/Alien (1979)/Alien (1979).mov", "Alien", Some(1979)),
        movie("Alien (1979)/alien.mkv", "alien", Some(1979)),
        movie("Metropolis.mkv", "Metropolis", None),
        movie("Nosferatu 1922.mkv", "Nosferatu", Some(1922)),
        movie(
            "Ghost in the Shell (1995) [Remastered].mkv",
            "Ghost in the Shell",
            Some(1995),
        ),
        movie_v(
            "Ghost in the Shell (1995) - Remastered.mkv",
            "Ghost in the Shell",
            1995,
            "Remastered",
        ),
        movie(
            "Howl's Moving Castle (2004).mkv",
            "Howl's Moving Castle",
            Some(2004),
        ),
        movie("WALL-E (2008).mkv", "WALL-E", Some(2008)),
        movie(
            "Spider-Man Into the Spider-Verse (2018).mkv",
            "Spider-Man Into the Spider-Verse",
            Some(2018),
        ),
        movie("Amélie (2001).mkv", "Amélie", Some(2001)),
        movie("東京物語 (1953).mkv", "東京物語", Some(1953)),
        movie(
            "Seven Samurai (1954) [Criterion].mkv",
            "Seven Samurai",
            Some(1954),
        ),
        movie(
            "Seven.Samurai.1954.REMUX.1080p.mkv",
            "Seven Samurai",
            Some(1954),
        ),
        movie("Paprika (2006) [Dub].ts", "Paprika", Some(2006)),
        movie(
            "Millennium Actress (2001).mpeg",
            "Millennium Actress",
            Some(2001),
        ),
        movie(
            "Tokyo Godfathers (2003).wmv",
            "Tokyo Godfathers",
            Some(2003),
        ),
        // 1.1 subtitles beside the video
        sub("Akira (1988).en.srt", Some("en"), false, false),
        sub("Akira (1988).en.forced.srt", Some("en"), true, false),
        sub("Akira (1988).en.sdh.srt", Some("en"), false, true),
        sub("Akira (1988).ja.srt", Some("ja"), false, false),
        sub("Akira (1988).pt-BR.srt", Some("pt-BR"), false, false),
        sub("Akira (1988).forced.en.srt", Some("en"), true, false),
        sub("Akira (1988).eng.srt", Some("eng"), false, false),
        sub("Akira (1988).srt", None, false, false),
        sub("Akira (1988).en.ass", Some("en"), false, false),
        sub(
            "Cowboy Bebop - s01e01 - Asteroid Blues.en.ass",
            Some("en"),
            false,
            false,
        ),
        sub("Cowboy Bebop - s01e01.de.ssa", Some("de"), false, false),
        sub("Season 01/Show - s01e02.fr.vtt", Some("fr"), false, false),
        sub("Show - s01e02.es.sdh.srt", Some("es"), false, true),
        sub("Show - s01e02.es.hi.srt", Some("es"), false, true),
        sub("Show.S01E02.1080p.WEB.en.srt", Some("en"), false, false),
        // 1.2 series
        ep(
            "Cowboy Bebop (1998)/Season 01/Cowboy Bebop - s01e01 - Asteroid Blues.mkv",
            "Cowboy Bebop",
            1,
            &[1],
            Some("Asteroid Blues"),
        ),
        ep(
            "Cowboy Bebop (1998)/Specials/Cowboy Bebop - s00e01 - Session XX.mkv",
            "Cowboy Bebop",
            0,
            &[1],
            Some("Session XX"),
        ),
        ep("Cowboy Bebop - s01e02.mkv", "Cowboy Bebop", 1, &[2], None),
        ep("Cowboy Bebop - S01E02.mkv", "Cowboy Bebop", 1, &[2], None),
        ep("Cowboy Bebop - s1e2.mkv", "Cowboy Bebop", 1, &[2], None),
        ep(
            "Cowboy Bebop s01e03 Honky Tonk Women.mkv",
            "Cowboy Bebop",
            1,
            &[3],
            Some("Honky Tonk Women"),
        ),
        ep(
            "Cowboy.Bebop.S01E04.Gateway.Shuffle.1080p.BluRay.x265-GRP.mkv",
            "Cowboy Bebop",
            1,
            &[4],
            Some("Gateway Shuffle"),
        ),
        ep("cowboy_bebop_s01e05.mkv", "cowboy bebop", 1, &[5], None),
        ep("Cowboy Bebop 1x06.mkv", "Cowboy Bebop", 1, &[6], None),
        ep(
            "Cowboy Bebop - 1x07 - Heavy Metal Queen.mkv",
            "Cowboy Bebop",
            1,
            &[7],
            Some("Heavy Metal Queen"),
        ),
        ep(
            "Cowboy Bebop Season 1 Episode 8.mkv",
            "Cowboy Bebop",
            1,
            &[8],
            None,
        ),
        ep(
            "Cowboy Bebop - Season 1 Episode 9 - Jamming with Edward.mkv",
            "Cowboy Bebop",
            1,
            &[9],
            Some("Jamming with Edward"),
        ),
        ep(
            "cowboy_bebop_season_1_episode_10.mkv",
            "cowboy bebop",
            1,
            &[10],
            None,
        ),
        ep("season_1_episode_11.mkv", "", 1, &[11], None),
        ep("Season 01/E05.mkv", "", 1, &[5], None),
        ep(
            "Cowboy Bebop (1998)/Season 01/E05.mkv",
            "Cowboy Bebop",
            1,
            &[5],
            None,
        ),
        ep(
            "Cowboy Bebop (1998)/Season 02/Episode 3.mkv",
            "Cowboy Bebop",
            2,
            &[3],
            None,
        ),
        ep(
            "Cowboy Bebop (1998)/Season 1/05.mkv",
            "Cowboy Bebop",
            1,
            &[5],
            None,
        ),
        ep(
            "Cowboy Bebop (1998)/Season 1/05 - Ballad of Fallen Angels.mkv",
            "Cowboy Bebop",
            1,
            &[5],
            Some("Ballad of Fallen Angels"),
        ),
        ep(
            "Cowboy Bebop (1998)/Season 00/Cowboy Bebop - S00E02.mkv",
            "Cowboy Bebop",
            0,
            &[2],
            None,
        ),
        // multi-episode
        ep("Show - s01e01-e02.mkv", "Show", 1, &[1, 2], None),
        ep(
            "Show - s01e01-e02 - Double Feature.mkv",
            "Show",
            1,
            &[1, 2],
            Some("Double Feature"),
        ),
        ep("Show - s01e01e02.mkv", "Show", 1, &[1, 2], None),
        ep("Show.S01E01-E03.mkv", "Show", 1, &[1, 2, 3], None),
        ep("Show - s01e05-06.mkv", "Show", 1, &[5, 6], None),
        ep(
            "Show - S02E09-E10 - Finale.mkv",
            "Show",
            2,
            &[9, 10],
            Some("Finale"),
        ),
        // absolute numbering
        abs("One Piece - 1000.mkv", "One Piece", &[1000]),
        abs("Naruto - 05.mkv", "Naruto", &[5]),
        abs("Naruto - 005.mkv", "Naruto", &[5]),
        abs(
            "[SubsPlease] Frieren - 12 (1080p) [ABCD1234].mkv",
            "Frieren",
            &[12],
        ),
        abs("[Group] Show Name - 03v2 [720p].mkv", "Show Name", &[3]),
        abs("Bleach - 366.mkv", "Bleach", &[366]),
        abs("Mob Psycho 100 - 05.mkv", "Mob Psycho 100", &[5]),
        abs("86 - 11.mkv", "86", &[11]),
        // tags are ignored for numbering
        ep(
            "[Group] Show - s01e03 [1080p][HEVC].mkv",
            "Show",
            1,
            &[3],
            None,
        ),
        ep("Show (2020) - s01e03 (1080p).mkv", "Show", 1, &[3], None),
        ep("Show [2020] s01e03.mkv", "Show", 1, &[3], None),
        // titles with digits and punctuation
        ep(
            "Steins;Gate - s01e01 - Turning Point.mkv",
            "Steins;Gate",
            1,
            &[1],
            Some("Turning Point"),
        ),
        ep("K-On! - s02e03.mkv", "K-On!", 2, &[3], None),
        ep("Re:Zero - s01e25.mkv", "Re:Zero", 1, &[25], None),
        ep(
            "Mob Psycho 100 - s02e12.mkv",
            "Mob Psycho 100",
            2,
            &[12],
            None,
        ),
        ep("86 - s01e01.mkv", "86", 1, &[1], None),
        ep("Spider-Man - s01e01.mkv", "Spider-Man", 1, &[1], None),
        ep("Fate Zero - s02e08.mkv", "Fate Zero", 2, &[8], None),
        ep("Show 1920x1080 s01e03.mkv", "Show 1920x1080", 1, &[3], None),
        // artwork
        art("poster.jpg", Poster),
        art("Poster.PNG", Poster),
        art("backdrop.jpg", Backdrop),
        art("fanart.jpg", Backdrop),
        art("logo.png", Logo),
        art("Cowboy Bebop (1998)/poster.webp", Poster),
        art("Cowboy Bebop (1998)/backdrop.jpeg", Backdrop),
        art("Cowboy Bebop (1998)/logo.png", Logo),
        art("Cowboy Bebop (1998)/season01-poster.jpg", SeasonPoster),
        art("Cowboy Bebop (1998)/season00-poster.jpg", SeasonPoster),
        art(
            "Cowboy Bebop (1998)/Season 01/Cowboy Bebop - s01e01 - Asteroid Blues-thumb.jpg",
            EpisodeThumb,
        ),
        art("Show - s01e01-thumb.png", EpisodeThumb),
        // unknown
        Case::Unknown("notes.txt".into()),
        Case::Unknown("random.jpg".into()),
        Case::Unknown("".into()),
        Case::Unknown("readme.nfo".into()),
        Case::Unknown("folder/data.bin".into()),
    ]
}

const SHOWS: &[&str] = &[
    "Cowboy Bebop",
    "Steins;Gate",
    "K-On!",
    "Re:Zero",
    "Mob Psycho 100",
    "86",
    "Spider-Man",
    "Fate Zero",
    "Death Note",
    "Fullmetal Alchemist",
    "Attack on Titan",
    "Neon Genesis Evangelion",
    "Hunter x Hunter",
    "Jujutsu Kaisen",
    "Demon Slayer",
    "One Punch Man",
    "My Hero Academia",
    "Samurai Champloo",
    "Trigun",
    "Monster",
    "Berserk",
    "Vinland Saga",
    "Haikyuu",
    "Clannad",
    "Gintama",
    "Bleach",
    "The Office",
    "Breaking Bad",
    "Dark",
    "Better Call Saul",
];

fn generated() -> usize {
    let mut n = 0;
    for (i, show) in SHOWS.iter().enumerate() {
        let s = (i % 5 + 1) as u32;
        let e = (i % 23 + 1) as u32;
        let ss = format!("{s:02}");
        let ee = format!("{e:02}");
        let dotted = show.replace(' ', ".");
        let under = show.replace(' ', "_");
        for p in [
            format!("{show} - s{ss}e{ee}.mkv"),
            format!("{show} - S{ss}E{ee}.mkv"),
            format!("{show} - s{ss}e{ee} - Pilot.mkv"),
            format!("{dotted}.S{ss}E{ee}.1080p.WEB-DL.x264-GRP.mkv"),
            format!("{under}_s{ss}e{ee}.mp4"),
            format!("{show}/Season {ss}/{show} - s{ss}e{ee}.mkv"),
            format!("{show} (2010)/Season {s}/{show} - s{ss}e{ee} - Title.mkv"),
            format!("[Grp] {show} - s{ss}e{ee} [1080p].mkv"),
            format!("{show} {s}x{ee}.avi"),
            format!("{show} - {s}x{ee} - Title.mkv"),
            format!("{show} Season {s} Episode {e}.mkv"),
        ] {
            let title = if p.contains('_') || p.contains('.') && !p.contains(' ') {
                // dotted and underscored names lose their separators: compare case-insensitively
                // below via the lowercase of the show.
                None
            } else {
                Some(*show)
            };
            let r = parse_path(&p);
            assert_eq!(r.media, Media::Episode, "{p}: {r:?}");
            assert_eq!(r.season, Some(s), "{p}: {r:?}");
            assert_eq!(r.episodes, vec![e], "{p}: {r:?}");
            match title {
                Some(t) => assert_eq!(r.title, t, "{p}: {r:?}"),
                None => assert_eq!(r.title, show.replace(['_', '.'], " "), "{p}: {r:?}"),
            }
            n += 1;
        }
        // absolute numbering for anime
        let abs_no = (i * 7 + 3) as u32;
        let p = format!("{show} - {abs_no:03}.mkv");
        let r = parse_path(&p);
        assert_eq!(r.episodes, vec![abs_no], "{p}: {r:?}");
        assert!(r.absolute, "{p}: {r:?}");
        n += 1;
    }
    n
}

const MOVIES: &[(&str, u16)] = &[
    ("Akira", 1988),
    ("Alien", 1979),
    ("Aliens", 1986),
    ("Heat", 1995),
    ("Her", 2013),
    ("Up", 2009),
    ("Coco", 2017),
    ("Dune", 2021),
    ("Drive", 2011),
    ("Gattaca", 1997),
    ("Inception", 2010),
    ("Interstellar", 2014),
    ("Jaws", 1975),
    ("Memento", 2000),
    ("Moon", 2009),
    ("Oldboy", 2003),
    ("Parasite", 2019),
    ("Psycho", 1960),
    ("Ran", 1985),
    ("Rashomon", 1950),
    ("Solaris", 1972),
    ("Stalker", 1979),
    ("Tenet", 2020),
    ("Titanic", 1997),
    ("Vertigo", 1958),
    ("Whiplash", 2014),
    ("Zodiac", 2007),
    ("The Thing", 1982),
    ("Blade Runner", 1982),
    ("Pulp Fiction", 1994),
];

fn generated_movies() -> usize {
    let mut n = 0;
    for (t, y) in MOVIES {
        let dotted = t.replace(' ', ".");
        let under = t.replace(' ', "_");
        for p in [
            format!("{t} ({y}).mkv"),
            format!("{t} ({y})/{t} ({y}).mp4"),
            format!("{dotted}.{y}.1080p.BluRay.x264-GRP.mkv"),
            format!("{under}_({y}).avi"),
            format!("{t} ({y}) [1080p].mkv"),
            format!("{t} ({y}) - Director's Cut.mkv"),
        ] {
            let r = parse_path(&p);
            assert_eq!(r.media, Media::Movie, "{p}: {r:?}");
            assert_eq!(r.year, Some(*y), "{p}: {r:?}");
            assert_eq!(r.title, t.replace(['_', '.'], " "), "{p}: {r:?}");
            if p.contains("Director") {
                assert_eq!(r.variant.as_deref(), Some("Director's Cut"), "{p}: {r:?}");
            }
            n += 1;
        }
        // Subtitles beside the movie.
        for (lang, flags) in [("en", ""), ("ja", ".forced"), ("fr", ".sdh")] {
            let p = format!("{t} ({y}).{lang}{flags}.srt");
            let r = parse_path(&p);
            assert_eq!(r.role, Role::Subtitle, "{p}: {r:?}");
            assert_eq!(r.language.as_deref(), Some(lang), "{p}: {r:?}");
            assert_eq!(r.forced, flags == ".forced", "{p}: {r:?}");
            assert_eq!(r.sdh, flags == ".sdh", "{p}: {r:?}");
            assert_eq!(r.year, Some(*y), "{p}: {r:?}");
            n += 1;
        }
    }
    n
}

#[test]
fn handwritten_names_parse_as_documented() {
    let cases = handwritten();
    assert!(cases.len() >= 100, "{}", cases.len());
    let failed = cases
        .iter()
        .filter(|c| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(c))).is_err())
        .count();
    assert_eq!(failed, 0, "{failed} fixtures failed, see above");
}

#[test]
fn the_suite_holds_at_least_300_names() {
    let total = handwritten().len() + generated() + generated_movies();
    assert!(total >= 300, "only {total} fixtures");
}

#[test]
fn hostile_input_never_panics() {
    for p in [
        "",
        "/",
        "////",
        "..",
        "../../etc/passwd",
        "\\\\server\\share\\x.mkv",
        ".mkv",
        "(.mkv",
        "[[[[]]]].mkv",
        "((((1999)))).mkv",
        "s99e9999999999999999999.mkv",
        "x.mkv.mkv.mkv",
        "a - 99999999999999999999.mkv",
        "\u{0}\u{1}.mkv",
        "S01E01-E999999.mkv",
        "S01E05-02.mkv",
    ] {
        let _ = parse_path(p);
    }
    let long = "a".repeat(100_000) + ".mkv";
    let _ = parse_path(&long);
    let many = "[x]".repeat(50_000);
    let _ = parse_path(&many);
}

#[test]
fn title_folders_split_the_year() {
    assert_eq!(
        parse_title_folder("Cowboy Bebop (1998)"),
        ("Cowboy Bebop".to_owned(), Some(1998))
    );
    assert_eq!(parse_title_folder("Akira"), ("Akira".to_owned(), None));
    assert_eq!(
        parse_title_folder("Spirited_Away (2001)"),
        ("Spirited Away".to_owned(), Some(2001))
    );
}
