//! Classify a key: which section, what it is called, where it sits in a series or an album.
//!
//! Folder names pick the section (`Movies/`, `Series/<show>/Season 01/`, `Anime/`, `Books/`, `Comics/`, `Manga/`,
//! `Music/<artist>/<album>/`); outside them the extension decides. A `.sideroom.json` sidecar in a folder (or any
//! folder above) overrides title, year, kind, tags, artist and album, or tells the indexer to ignore the folder.
//! Video names go through a [`NameParser`] (in production `sr_media::naming`), so the S01E02, year, group-tag and
//! absolute-number rules are the ones submissions already use; book and music names are read here.

use std::collections::{BTreeMap, HashMap};

use crate::scopes::Kind;

/// Longest key looked at (the same cap as `safe_key`).
const MAX_KEY: usize = 1024;

/// What the file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Class {
    /// A film (or a feature-length anime).
    Movie,
    /// One episode of a series or anime.
    Episode,
    /// A book (EPUB, PDF).
    Book,
    /// A comic (CBZ, CBR).
    Comic,
    /// A manga volume or chapter (CBZ, CBR, EPUB, PDF).
    Manga,
    /// An audio track.
    Track,
    /// A game ROM under `emu/roms/` (wave 37).
    Rom,
    /// A cover, poster or backdrop.
    Artwork,
    /// A subtitle file.
    Subtitle,
    /// A `.sideroom.json`.
    Sidecar,
    /// Anything else; not stored.
    #[default]
    Other,
}

impl Class {
    /// The stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Episode => "episode",
            Self::Book => "book",
            Self::Comic => "comic",
            Self::Manga => "manga",
            Self::Track => "track",
            Self::Rom => "rom",
            Self::Artwork => "artwork",
            Self::Subtitle => "subtitle",
            Self::Sidecar => "sidecar",
            Self::Other => "other",
        }
    }
    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "movie" => Self::Movie,
            "episode" => Self::Episode,
            "book" => Self::Book,
            "comic" => Self::Comic,
            "manga" => Self::Manga,
            "track" => Self::Track,
            "rom" => Self::Rom,
            "artwork" => Self::Artwork,
            "subtitle" => Self::Subtitle,
            "sidecar" => Self::Sidecar,
            "other" => Self::Other,
            _ => return None,
        })
    }
    /// The shelf the class belongs to; `None` for companions.
    pub fn kind(self) -> Option<Kind> {
        match self {
            Self::Movie | Self::Episode => Some(Kind::Cinema),
            Self::Book | Self::Comic | Self::Manga => Some(Kind::Library),
            Self::Track => Some(Kind::Music),
            Self::Rom => Some(Kind::Games),
            _ => None,
        }
    }
}

/// Which artwork a picture is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtRole {
    /// Poster or cover.
    Poster,
    /// Backdrop or fan art.
    Backdrop,
    /// Logo.
    Logo,
}

/// What a video-name parser says about a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoRole {
    /// A video file.
    #[default]
    Video,
    /// A subtitle.
    Subtitle,
    /// Anything else.
    Other,
}

/// The facts a video-name parser reads from a path relative to its section folder.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VideoFacts {
    /// Video, subtitle or other.
    pub role: VideoRole,
    /// The name says it is an episode (S01E02, 1x02, "Season 1 Episode 2", a numbered anime file).
    pub episode: bool,
    /// Movie title, or the show's title for an episode.
    pub title: String,
    /// Release year.
    pub year: Option<u16>,
    /// Season number.
    pub season: Option<u32>,
    /// Episode numbers (more than one for a multi-episode file).
    pub episodes: Vec<u32>,
    /// Episode title, when the name carries one.
    pub episode_title: Option<String>,
    /// Numbered `Show - 05` without a season.
    pub absolute: bool,
    /// Season 0 or a specials folder.
    pub special: bool,
    /// Alternate cut (`Director's Cut`).
    pub variant: Option<String>,
    /// Subtitle language as written.
    pub language: Option<String>,
    /// Bracket and parenthesis tags and quality words.
    pub tags: Vec<String>,
}

/// Parses video names (`sr_media::naming::parse_path` behind an adapter).
pub trait NameParser: Send + Sync {
    /// `path` is relative to the section folder and uses `/`.
    fn video(&self, path: &str) -> VideoFacts;
}

/// Per-run switches.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// `library.ingest.rar_enabled`: CBR files are indexed only when set.
    pub rar_enabled: bool,
}

/// The overrides of one `.sideroom.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sidecar {
    /// Title of the series, film, album or book the folder holds.
    pub title: Option<String>,
    /// Year.
    pub year: Option<u16>,
    /// `movie`, `series`, `anime`, `book`, `comic`, `manga` or `music`: forces the section.
    pub kind: Option<String>,
    /// Extra tags.
    pub tags: Vec<String>,
    /// Artist of the folder's tracks.
    pub artist: Option<String>,
    /// Album of the folder's tracks.
    pub album: Option<String>,
    /// Skip everything under the folder.
    pub ignore: bool,
}

/// Sidecars by folder: the key is `""` for the root or `"a/b/"` with a trailing slash.
pub type SidecarMap = BTreeMap<String, Sidecar>;

/// Whether a file name is the folder sidecar.
pub fn is_sidecar_name(name: &str) -> bool {
    name == super::SIDECAR_NAME
}

/// The classifier's answer for one key.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Classified {
    /// Mount-relative key.
    pub key: String,
    /// Size in bytes (from the listing).
    pub size: u64,
    /// What it is.
    pub class: Class,
    /// Lower-case extension.
    pub container: String,
    /// Plays in a browser without conversion.
    pub web_native: bool,
    /// Anime (from the folder or the sidecar).
    pub anime: bool,
    /// Not stored (`reason` says why).
    pub ignored: bool,
    /// Why it is ignored.
    pub reason: Option<&'static str>,
    /// Movie title, or an episode's show title, or a chapter's or track's own title.
    pub title: String,
    /// Year.
    pub year: Option<u16>,
    /// The series (Library) or show (Cinema) a file belongs to, as displayed.
    pub series: Option<String>,
    /// The series' stable grouping key, `Section/Folder`; films use their own key.
    pub shelf_key: Option<String>,
    /// Season (episodes).
    pub season: Option<u32>,
    /// Episode numbers.
    pub episodes: Vec<u32>,
    /// Episode title.
    pub episode_title: Option<String>,
    /// Absolute numbering.
    pub absolute: bool,
    /// Specials.
    pub special: bool,
    /// Alternate cut.
    pub variant: Option<String>,
    /// Chapter, volume or issue number.
    pub number: Option<f64>,
    /// Track artist.
    pub artist: String,
    /// Track album.
    pub album: String,
    /// Track number.
    pub track_no: Option<u32>,
    /// Disc number.
    pub disc_no: Option<u32>,
    /// Artwork role (artwork files).
    pub art: Option<ArtRole>,
    /// Subtitle language.
    pub language: Option<String>,
    /// Lower-case tags.
    pub tags: Vec<String>,
    /// The poster or cover found beside it.
    pub cover_key: Option<String>,
    /// The backdrop found beside it.
    pub backdrop_key: Option<String>,
    /// The browser-made playable copy of a video that does not play as it is: `converted/<stem>.mp4` (or
    /// `.webm`) found in the same listing (BYOS-CNV-001, migration 1480). Shelves play it instead of `key`.
    pub playable_key: Option<String>,
    /// The platform of a ROM (`nes`, `gba`, ...); `None` when the extension cannot tell (a `.bin`).
    pub platform: Option<&'static str>,
    /// The emulator core that plays it (a directory name under `/emu/cores/`).
    pub core: Option<&'static str>,
}

impl Classified {
    fn is_library(&self) -> bool {
        matches!(self.class, Class::Book | Class::Comic | Class::Manga)
    }
}

/// Folders that hold copies made from other objects of the mount: the browser converter's `converted/`
/// (BYOS-CNV-001) and the worker's `transcoded/` (BYOS-CNV-003). Their files are never shelf items of their own.
pub const DERIVED_PREFIXES: &[&str] = &["converted/", "transcoded/"];

/// Whether `key` is a copy the converter or the worker made (see [`DERIVED_PREFIXES`]).
pub fn is_derived(key: &str) -> bool {
    DERIVED_PREFIXES.iter().any(|p| key.starts_with(p))
}

/// The keys the browser converter writes for `original` (`Movies/Film.mkv` to `converted/Movies/Film.mp4`, then
/// `.webm`), in preference order. Mirrors `convertedKey` in `apps/web/src/convert/plan.ts`.
pub fn converted_candidates(original: &str) -> [String; 2] {
    let slash = original.rfind('/').map_or(0, |i| i + 1);
    let stem = match original.rfind('.') {
        Some(dot) if dot > slash => &original[..dot],
        _ => original,
    };
    [
        format!("converted/{stem}.mp4"),
        format!("converted/{stem}.webm"),
    ]
}

const VIDEO_EXT: &[&str] = &[
    "mkv", "mp4", "m4v", "mov", "avi", "webm", "ts", "m2ts", "mpg", "mpeg", "ogv", "wmv", "flv",
];
const AUDIO_EXT: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "m4a", "aac", "wav", "wma", "aiff", "alac", "mka",
];
const BOOK_EXT: &[&str] = &["epub", "pdf", "cbz", "cbr"];
const IMG_EXT: &[&str] = &["jpg", "jpeg", "png", "webp"];
const SUB_EXT: &[&str] = &["srt", "ass", "ssa", "vtt", "sub"];

/// Containers a browser plays without conversion, by extension (the header sniff refines MP4).
pub fn web_native(ext: &str) -> bool {
    matches!(
        ext,
        "mp4" | "m4v" | "webm" | "mp3" | "flac" | "ogg" | "oga" | "opus" | "m4a" | "aac" | "wav"
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Movies,
    Series,
    Anime,
    Books,
    Comics,
    Manga,
    Music,
}

fn section_of(folder: &str) -> Option<Section> {
    Some(match folder.to_ascii_lowercase().as_str() {
        "movies" | "movie" | "films" | "film" => Section::Movies,
        "series" | "shows" | "tv" | "tv shows" | "tv series" => Section::Series,
        "anime" => Section::Anime,
        "books" | "ebooks" => Section::Books,
        "comics" => Section::Comics,
        "manga" => Section::Manga,
        "music" | "audio" => Section::Music,
        _ => return None,
    })
}

fn section_from_kind(kind: &str) -> Option<Section> {
    section_of(match kind {
        "movie" => "movies",
        other => other,
    })
}

fn ext_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() => e.to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn stem_of(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

fn spaced(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c == '_' || c == '.' { ' ' } else { c })
        .collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `Name (1999)` into `("Name", Some(1999))`.
fn split_year(s: &str) -> (String, Option<u16>) {
    let s = s.trim();
    if let Some(open) = s.rfind('(')
        && s.ends_with(')')
    {
        let inner = &s[open + 1..s.len() - 1];
        if inner.len() == 4
            && let Ok(y) = inner.parse::<u16>()
            && (1000..=2200).contains(&y)
        {
            let name = s[..open].trim().trim_end_matches(['-', ' ']).to_owned();
            return (name, Some(y));
        }
    }
    (s.to_owned(), None)
}

/// The number after a marker word: `ch 12`, `chapter12.5`, `vol. 3`, `#7`, `issue 4`. The earliest marker wins.
fn marked_number(stem: &str, markers: &[&str]) -> Option<f64> {
    let lower = stem.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut best: Option<(usize, f64)> = None;
    for m in markers {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(m) {
            let at = from + pos;
            from = at + m.len();
            // The marker must start a word (not the middle of "archive").
            if at > 0 && bytes[at - 1].is_ascii_alphanumeric() && *m != "#" {
                continue;
            }
            let mut i = at + m.len();
            while i < bytes.len() && matches!(bytes[i], b' ' | b'.' | b'_' | b'-' | b'#' | b':') {
                i += 1;
            }
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                continue;
            }
            let mut end = i;
            if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
                end = i + 1;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
            if let Ok(n) = lower[start..end].parse::<f64>()
                && best.is_none_or(|(p, _)| at < p)
            {
                best = Some((at, n));
            }
        }
    }
    best.map(|(_, n)| n)
}

/// Chapter, issue or volume number of a library file name. Chapters win over volumes; a bare trailing number is
/// taken when nothing is marked; years are not numbers.
pub fn chapter_number(stem: &str) -> Option<f64> {
    if let Some(n) = marked_number(stem, &["chapter", "ch", "c", "issue", "#", "no"]) {
        return Some(n);
    }
    if let Some(n) = marked_number(stem, &["volume", "vol", "v", "tome", "t"]) {
        return Some(n);
    }
    let digits: Vec<&str> = stem
        .split(|c: char| !c.is_ascii_digit())
        .filter(|d| !d.is_empty())
        .collect();
    let last = digits.last()?;
    if last.len() == 4
        && let Ok(y) = last.parse::<u16>()
        && (1900..=2100).contains(&y)
    {
        return None;
    }
    last.parse::<f64>().ok()
}

/// Strip bracket tags, volume and chapter markers and trailing numbers from a file stem: the series name of a
/// loose file.
fn series_from_stem(stem: &str) -> String {
    let mut s = spaced(stem);
    let mut out = String::new();
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    s = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = s.to_ascii_lowercase();
    for m in [
        " vol", " v", " ch", " chapter", " #", " issue", " c", " tome",
    ] {
        if let Some(p) = lower.rfind(m) {
            let rest = lower[p + m.len()..].trim_start_matches(['.', ' ', '_']);
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                s.truncate(p);
                break;
            }
        }
    }
    let t = s.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == '-' || c == '.');
    if t.is_empty() {
        s.trim().to_owned()
    } else {
        t.trim().to_owned()
    }
}

fn art_role(name: &str) -> Option<ArtRole> {
    let ext = ext_of(name);
    if !IMG_EXT.contains(&ext.as_str()) {
        return None;
    }
    let stem = stem_of(name).to_ascii_lowercase();
    match stem.as_str() {
        "poster" | "cover" | "folder" | "front" | "thumb" | "albumart" => Some(ArtRole::Poster),
        "backdrop" | "fanart" | "background" | "banner" => Some(ArtRole::Backdrop),
        "logo" | "clearlogo" => Some(ArtRole::Logo),
        _ => None,
    }
}

fn is_junk(name: &str) -> bool {
    (name.starts_with('.') && name != super::SIDECAR_NAME)
        || name.eq_ignore_ascii_case("thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
}

fn is_disc_dir(name: &str) -> Option<u32> {
    let l = name.to_ascii_lowercase();
    let rest = l
        .strip_prefix("disc")
        .or_else(|| l.strip_prefix("disk"))
        .or_else(|| l.strip_prefix("cd"))?;
    let n = rest.trim_matches([' ', '_', '-', '.']);
    n.parse().ok()
}

/// `(disc, track, title)` from a track file stem: `07 - Title`, `1-07 Title`, `07. Title`, `Title`.
fn track_prefix(stem: &str) -> (Option<u32>, Option<u32>, String) {
    let s = stem.trim();
    let digits = |t: &str| t.chars().take_while(char::is_ascii_digit).count();
    let n1 = digits(s);
    if n1 == 0 || n1 > 3 {
        return (None, None, s.to_owned());
    }
    let first: u32 = s[..n1].parse().unwrap_or(0);
    let rest = &s[n1..];
    // `1-07 Title`: a dash straight into digits is disc-track.
    if let Some(after) = rest.strip_prefix('-') {
        let n2 = digits(after);
        if (1..=3).contains(&n2) {
            let t: u32 = after[..n2].parse().unwrap_or(0);
            let title = after[n2..].trim_start_matches(['.', ' ', '-', '_']).trim();
            return (Some(first), Some(t), title.to_owned());
        }
    }
    if !(rest.is_empty() || rest.starts_with(['.', ' ', '-', '_', ')'])) {
        return (None, None, s.to_owned());
    }
    let title = rest.trim_start_matches(['.', ' ', '-', '_', ')']).trim();
    if title.is_empty() {
        return (None, Some(first), s.to_owned());
    }
    (None, Some(first), title.to_owned())
}

fn merged_sidecar(key_dirs: &[&str], sidecars: &SidecarMap) -> Sidecar {
    // Nearest folder first; each field takes the nearest value, `ignore` and tags accumulate.
    let mut out = Sidecar::default();
    let mut prefixes: Vec<String> = vec![String::new()];
    let mut p = String::new();
    for d in key_dirs {
        p.push_str(d);
        p.push('/');
        prefixes.push(p.clone());
    }
    for pre in prefixes.iter().rev() {
        let Some(sc) = sidecars.get(pre) else {
            continue;
        };
        out.title = out.title.or_else(|| sc.title.clone());
        out.year = out.year.or(sc.year);
        out.kind = out.kind.or_else(|| sc.kind.clone());
        out.artist = out.artist.or_else(|| sc.artist.clone());
        out.album = out.album.or_else(|| sc.album.clone());
        out.ignore |= sc.ignore;
        for t in &sc.tags {
            if !out.tags.contains(t) {
                out.tags.push(t.clone());
            }
        }
    }
    out
}

fn clean_tag(t: &str) -> Option<String> {
    let t = t.trim().to_ascii_lowercase();
    (!t.is_empty() && t.chars().count() <= 32 && !t.contains(['/', '\\'])).then_some(t)
}

fn ignored(mut c: Classified, why: &'static str) -> Classified {
    c.ignored = true;
    c.reason = Some(why);
    c
}

/// Classify one key. `size` is stored as given.
pub fn classify(
    key: &str,
    size: u64,
    parser: &dyn NameParser,
    sidecars: &SidecarMap,
    opts: Options,
) -> Classified {
    let mut c = Classified {
        key: key.to_owned(),
        size,
        ..Classified::default()
    };
    if key.is_empty() || key.len() > MAX_KEY || key.ends_with('/') {
        return ignored(c, "folder or bad key");
    }
    let segs: Vec<&str> = key.split('/').filter(|s| !s.is_empty()).collect();
    let Some((name, dirs)) = segs.split_last() else {
        return ignored(c, "empty");
    };
    if is_sidecar_name(name) {
        c.class = Class::Sidecar;
        return ignored(c, "sidecar");
    }
    if is_junk(name) {
        return ignored(c, "junk");
    }
    if is_derived(key) {
        return ignored(c, "playable copy of another object");
    }
    let ext = ext_of(name);
    c.container.clone_from(&ext);
    let sc = merged_sidecar(dirs, sidecars);
    if sc.ignore {
        return ignored(c, "ignored by sidecar");
    }

    // ROMs: only under `emu/roms/`; everything else under `emu/` (saves, BIOS) is never cataloged.
    if key.starts_with(crate::scopes::EMU_PREFIX) {
        if !key.starts_with(super::rom::ROM_PREFIX) || !super::rom::is_rom_ext(&ext) {
            return ignored(c, "not a ROM");
        }
        c.class = Class::Rom;
        c.title = spaced(stem_of(name));
        let p = super::rom::by_extension(&ext);
        c.platform = p.map(super::rom::Platform::as_str);
        c.core = p.map(super::rom::Platform::core);
        return c;
    }

    // Section: the sidecar's kind, else the first folder, else the extension.
    let by_folder = dirs.first().and_then(|d| section_of(d));
    let forced = sc.kind.as_deref().and_then(section_from_kind);
    let in_section_folder = by_folder.is_some() && forced.is_none();
    let section = forced.or(by_folder);
    // Path below the section folder.
    let rel: Vec<&str> = if in_section_folder {
        segs[1..].to_vec()
    } else {
        segs.clone()
    };
    let rel_dirs = &rel[..rel.len().saturating_sub(1)];
    let section_name = if in_section_folder {
        dirs[0].to_ascii_lowercase()
    } else {
        String::new()
    };

    if let Some(role) = art_role(name) {
        c.class = Class::Artwork;
        c.art = Some(role);
        return c;
    }
    if SUB_EXT.contains(&ext.as_str()) {
        c.class = Class::Subtitle;
        return c;
    }

    let is_video = VIDEO_EXT.contains(&ext.as_str());
    let is_audio = AUDIO_EXT.contains(&ext.as_str());
    let is_book = BOOK_EXT.contains(&ext.as_str());
    c.web_native = web_native(&ext);
    let video_ok = matches!(
        section,
        None | Some(Section::Movies | Section::Series | Section::Anime)
    );
    let book_ok = matches!(
        section,
        None | Some(Section::Books | Section::Comics | Section::Manga)
    );
    let audio_ok = matches!(section, None | Some(Section::Music));

    if is_video && video_ok {
        let f = parser.video(&rel.join("/"));
        if f.role != VideoRole::Video {
            return ignored(c, "not a video");
        }
        if section == Some(Section::Series) && !f.episode {
            return ignored(c, "no episode number in a Series folder");
        }
        c.anime = section == Some(Section::Anime) || sc.kind.as_deref() == Some("anime");
        c.tags = f.tags.iter().filter_map(|t| clean_tag(t)).collect();
        c.title = sc.title.clone().unwrap_or_else(|| f.title.clone());
        c.year = sc.year.or(f.year);
        if c.title.trim().is_empty() {
            c.title = spaced(stem_of(name));
        }
        let is_ep = section != Some(Section::Movies) && f.episode;
        if is_ep {
            c.class = Class::Episode;
            c.series = Some(c.title.clone());
            c.season = Some(f.season.unwrap_or(1));
            c.episodes.clone_from(&f.episodes);
            c.episode_title.clone_from(&f.episode_title);
            c.absolute = f.absolute;
            c.special = f.special;
            c.shelf_key = Some(match (section_name.is_empty(), rel_dirs.first()) {
                (false, Some(g)) => format!("{section_name}/{g}"),
                (true, Some(g)) => (*g).to_owned(),
                _ => format!("{section_name}/{}", c.title.to_ascii_lowercase()),
            });
        } else {
            c.class = Class::Movie;
            c.variant.clone_from(&f.variant);
            c.shelf_key = Some(key.to_owned());
        }
        if c.anime && !c.tags.iter().any(|t| t == "anime") {
            c.tags.push("anime".into());
        }
    } else if is_book && book_ok {
        if ext == "cbr" && !opts.rar_enabled {
            return ignored(c, "CBR needs library.ingest.rar_enabled");
        }
        c.class = match section {
            Some(Section::Manga) => Class::Manga,
            Some(Section::Comics) => Class::Comic,
            Some(Section::Books) => Class::Book,
            _ if ext == "cbz" || ext == "cbr" => Class::Comic,
            _ => Class::Book,
        };
        let stem = stem_of(name);
        let (series, year) = match rel_dirs.first() {
            Some(d) => split_year(&spaced(d)),
            None => (series_from_stem(stem), None),
        };
        c.series = Some(sc.title.clone().unwrap_or(series));
        c.year = sc.year.or(year);
        c.title = spaced(stem);
        c.number = chapter_number(stem);
        if c.class == Class::Book && rel_dirs.is_empty() {
            c.number = Some(1.0);
            c.series = Some(sc.title.clone().unwrap_or_else(|| spaced(stem)));
        }
        let group = rel_dirs
            .first()
            .map_or_else(|| stem.to_owned(), |d| (*d).to_owned());
        let sname = if section_name.is_empty() {
            c.class.as_str().to_owned()
        } else {
            section_name.clone()
        };
        c.shelf_key = Some(format!("{sname}/{group}"));
    } else if is_audio && audio_ok {
        c.class = Class::Track;
        let mut d: Vec<&str> = rel_dirs.to_vec();
        let mut disc = None;
        if let Some(last) = d.last()
            && let Some(n) = is_disc_dir(last)
        {
            disc = Some(n);
            d.pop();
        }
        let (artist_dir, album_dir) = match d.len() {
            0 => (None, None),
            1 => {
                let a = spaced(d[0]);
                match a.split_once(" - ") {
                    Some((ar, al)) => (Some(ar.trim().to_owned()), Some(al.trim().to_owned())),
                    None => (Some(a), None),
                }
            }
            _ => (Some(spaced(d[0])), Some(spaced(d[1]))),
        };
        let (td, tn, rest) = track_prefix(stem_of(name));
        let mut artist = artist_dir.unwrap_or_default();
        let mut title = spaced(&rest);
        if artist.is_empty()
            && let Some((ar, t)) = title.clone().split_once(" - ")
        {
            artist = ar.trim().to_owned();
            title = t.trim().to_owned();
        }
        let (album, year) = split_year(&album_dir.unwrap_or_default());
        c.artist = sc.artist.clone().unwrap_or(artist);
        c.album = sc.album.clone().unwrap_or(album);
        c.year = sc.year.or(year);
        c.title = if title.is_empty() {
            spaced(stem_of(name))
        } else {
            title
        };
        c.track_no = tn;
        c.disc_no = td.or(disc);
    } else {
        return ignored(c, "not a media file");
    }
    for t in &sc.tags {
        if let Some(t) = clean_tag(t)
            && !c.tags.contains(&t)
        {
            c.tags.push(t);
        }
    }
    c
}

/// Classify a whole listing, then bind covers and backdrops found beside their folders and number loose chapters.
/// The result keeps listing order.
pub fn classify_all(
    listing: &[(String, u64)],
    parser: &dyn NameParser,
    sidecars: &SidecarMap,
    opts: Options,
) -> Vec<Classified> {
    let mut out: Vec<Classified> = listing
        .iter()
        .map(|(k, s)| classify(k, *s, parser, sidecars, opts))
        .collect();
    // Artwork by folder.
    let mut art: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for c in out.iter().filter(|c| c.class == Class::Artwork) {
        let dir = c.key.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
        let e = art.entry(dir).or_default();
        match c.art {
            Some(ArtRole::Poster) => {
                e.0.get_or_insert_with(|| c.key.clone());
            }
            Some(ArtRole::Backdrop) => {
                e.1.get_or_insert_with(|| c.key.clone());
            }
            _ => {}
        }
    }
    for c in out
        .iter_mut()
        .filter(|c| !c.ignored && c.class.kind().is_some())
    {
        // A film or a book takes art from its own folder only; an episode, chapter or track walks up to its
        // show, series or artist folder. Never the mount root.
        let single = matches!(c.class, Class::Movie | Class::Book);
        let mut dir = c.key.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
        let mut hops = 0;
        while !dir.is_empty() && hops < 3 {
            if let Some((p, b)) = art.get(&dir) {
                c.cover_key = c.cover_key.take().or_else(|| p.clone());
                c.backdrop_key = c.backdrop_key.take().or_else(|| b.clone());
                if c.cover_key.is_some() {
                    break;
                }
            }
            if single {
                break;
            }
            dir = dir
                .rsplit_once('/')
                .map_or_else(String::new, |(d, _)| d.to_owned());
            hops += 1;
        }
    }
    // Loose chapters without a number follow the numbered ones of their series, in key order.
    let mut by_series: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, c) in out.iter().enumerate() {
        if !c.ignored
            && c.is_library()
            && let Some(k) = &c.shelf_key
        {
            by_series.entry(k.clone()).or_default().push(i);
        }
    }
    for idxs in by_series.values() {
        let mut next = idxs
            .iter()
            .filter_map(|&i| out[i].number)
            .fold(0.0_f64, f64::max);
        let mut missing: Vec<usize> = idxs
            .iter()
            .copied()
            .filter(|&i| out[i].number.is_none())
            .collect();
        missing.sort_by(|&a, &b| out[a].key.cmp(&out[b].key));
        for i in missing {
            next += 1.0;
            out[i].number = Some(next);
        }
    }
    // A video that does not play as it is takes the converted copy found beside it in this listing.
    let keys: std::collections::HashSet<&str> = listing.iter().map(|(k, _)| k.as_str()).collect();
    for c in out
        .iter_mut()
        .filter(|c| !c.ignored && !c.web_native && c.class.kind() == Some(Kind::Cinema))
    {
        c.playable_key = converted_candidates(&c.key)
            .into_iter()
            .find(|k| keys.contains(k.as_str()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the real parser: `Show S01E02 ...` style only.
    struct Fake;
    impl NameParser for Fake {
        fn video(&self, path: &str) -> VideoFacts {
            let file = path.rsplit('/').next().unwrap_or(path);
            let mut f = VideoFacts {
                title: spaced(stem_of(file)),
                ..VideoFacts::default()
            };
            if let Some(p) = file.find(" S01E") {
                f.episode = true;
                f.title = file[..p].to_owned();
                f.season = Some(1);
                f.episodes = vec![file[p + 5..p + 7].parse().unwrap_or(1)];
            }
            f
        }
    }

    fn c(key: &str) -> Classified {
        classify(key, 1, &Fake, &SidecarMap::new(), Options::default())
    }

    #[test]
    fn folders_pick_the_section() {
        assert_eq!(c("Movies/Alien.mp4").class, Class::Movie);
        assert_eq!(c("Series/Show/Show S01E02.mkv").class, Class::Episode);
        let a = c("Anime/Show/Show S01E02.mkv");
        assert!(a.anime && a.class == Class::Episode);
        assert!(a.tags.contains(&"anime".to_owned()));
        assert_eq!(c("Anime/Film.mkv").class, Class::Movie);
        assert_eq!(c("Manga/Berserk/Vol 03.cbz").class, Class::Manga);
        assert_eq!(c("Comics/Saga/Saga #012.cbz").class, Class::Comic);
        assert_eq!(c("Books/Dune.epub").class, Class::Book);
        assert_eq!(c("Music/Artist/Album/01 - Song.flac").class, Class::Track);
        assert_eq!(c("Notes/readme.txt").class, Class::Other);
        assert!(c("Notes/readme.txt").ignored);
    }

    #[test]
    fn extensions_decide_outside_the_folders() {
        assert_eq!(c("stuff/a.cbz").class, Class::Comic);
        assert_eq!(c("stuff/a.epub").class, Class::Book);
        assert_eq!(c("stuff/a.mp3").class, Class::Track);
        assert_eq!(c("stuff/Show S01E03.mp4").class, Class::Episode);
        assert_eq!(c("stuff/Film.mp4").class, Class::Movie);
    }

    #[test]
    fn a_series_folder_file_without_an_episode_number_is_left_out() {
        let x = c("Series/Show/extras/behind.mkv");
        assert!(x.ignored);
    }

    #[test]
    fn a_converted_copy_is_the_playable_key_of_its_original_and_no_title_of_its_own() {
        let listing: Vec<(String, u64)> = [
            "Movies/Film (2019).mkv",
            "converted/Movies/Film (2019).mp4",
            "Movies/Other.avi",
            "converted/Movies/Other.webm",
            "Movies/Native.mp4",
            "converted/Movies/Native.mp4",
            "Movies/Alone.mkv",
            "transcoded/Movies/Alone.mkv/video.mp4",
        ]
        .iter()
        .map(|k| ((*k).to_owned(), 1))
        .collect();
        let all = classify_all(&listing, &Fake, &SidecarMap::new(), Options::default());
        let by = |k: &str| all.iter().find(|c| c.key == k).unwrap();
        assert_eq!(
            by("Movies/Film (2019).mkv").playable_key.as_deref(),
            Some("converted/Movies/Film (2019).mp4")
        );
        assert_eq!(
            by("Movies/Other.avi").playable_key.as_deref(),
            Some("converted/Movies/Other.webm")
        );
        // A file that plays as it is keeps its own key; one with no copy has none (the worker's output comes
        // from `storage_object_transcodes`, not from the listing).
        assert_eq!(by("Movies/Native.mp4").playable_key, None);
        assert_eq!(by("Movies/Alone.mkv").playable_key, None);
        // The copies themselves are never shelf items.
        for k in [
            "converted/Movies/Film (2019).mp4",
            "converted/Movies/Other.webm",
            "transcoded/Movies/Alone.mkv/video.mp4",
        ] {
            assert!(by(k).ignored, "{k}");
        }
        assert_eq!(
            converted_candidates("Film"),
            [
                "converted/Film.mp4".to_owned(),
                "converted/Film.webm".to_owned()
            ]
        );
        assert_eq!(
            converted_candidates(".hidden/a")[0],
            "converted/.hidden/a.mp4"
        );
    }

    #[test]
    fn web_native_follows_the_container() {
        assert!(c("Movies/a.mp4").web_native);
        assert!(c("Movies/a.webm").web_native);
        assert!(!c("Movies/a.mkv").web_native);
        assert!(c("Music/x.flac").web_native);
        assert!(!c("Music/x.wma").web_native);
    }

    #[test]
    fn cbr_needs_the_switch() {
        assert!(c("Comics/A/1.cbr").ignored);
        let on = classify(
            "Comics/A/1.cbr",
            1,
            &Fake,
            &SidecarMap::new(),
            Options { rar_enabled: true },
        );
        assert!(!on.ignored);
    }

    #[test]
    fn chapter_numbers() {
        assert_eq!(chapter_number("Berserk Vol 03"), Some(3.0));
        assert_eq!(chapter_number("One Piece c1012.5"), Some(1012.5));
        assert_eq!(chapter_number("Saga #012"), Some(12.0));
        assert_eq!(chapter_number("Chapter 7 - Vol 2"), Some(7.0));
        assert_eq!(chapter_number("Dune (1965)"), None);
        assert_eq!(chapter_number("Archive"), None);
        assert_eq!(chapter_number("Title 42"), Some(42.0));
    }

    #[test]
    fn loose_names_give_their_series() {
        assert_eq!(series_from_stem("Berserk Vol 03 [Digital]"), "Berserk");
        assert_eq!(series_from_stem("Saga #012 (2012)"), "Saga");
        assert_eq!(series_from_stem("Dune"), "Dune");
    }

    #[test]
    fn music_names() {
        let t = c("Music/Daft Punk/Discovery (2001)/03 - Digital Love.flac");
        assert_eq!(
            (t.artist.as_str(), t.album.as_str(), t.title.as_str()),
            ("Daft Punk", "Discovery", "Digital Love")
        );
        assert_eq!((t.track_no, t.year), (Some(3), Some(2001)));
        let d = c("Music/A/B/Disc 2/1-07 Song.mp3");
        assert_eq!((d.disc_no, d.track_no), (Some(1), Some(7)));
        let d2 = c("Music/A/B/CD2/07. Song.mp3");
        assert_eq!((d2.disc_no, d2.track_no), (Some(2), Some(7)));
        let flat = c("Music/Queen - Bohemian Rhapsody.mp3");
        assert_eq!(
            (flat.artist.as_str(), flat.title.as_str()),
            ("Queen", "Bohemian Rhapsody")
        );
    }

    #[test]
    fn a_sidecar_overrides_and_ignores() {
        let mut m = SidecarMap::new();
        m.insert(
            "Series/Odd/".into(),
            Sidecar {
                title: Some("Fixed Name".into()),
                year: Some(1999),
                kind: Some("anime".into()),
                tags: vec!["Cult".into()],
                ..Sidecar::default()
            },
        );
        m.insert(
            "Movies/Skip/".into(),
            Sidecar {
                ignore: true,
                ..Sidecar::default()
            },
        );
        let x = classify(
            "Series/Odd/Odd S01E02.mkv",
            1,
            &Fake,
            &m,
            Options::default(),
        );
        assert_eq!(x.title, "Fixed Name");
        assert_eq!(x.year, Some(1999));
        assert!(x.anime);
        assert!(x.tags.contains(&"cult".to_owned()) && x.tags.contains(&"anime".to_owned()));
        assert!(classify("Movies/Skip/a.mp4", 1, &Fake, &m, Options::default()).ignored);
        assert!(!classify("Movies/Keep/a.mp4", 1, &Fake, &m, Options::default()).ignored);
    }

    #[test]
    fn covers_bind_by_folder_and_loose_chapters_get_numbers() {
        let listing: Vec<(String, u64)> = [
            "Manga/Berserk/cover.jpg",
            "Manga/Berserk/Vol 01.cbz",
            "Manga/Berserk/Extra.cbz",
            "Manga/Berserk/Vol 02.cbz",
            "Series/Show/poster.png",
            "Series/Show/backdrop.jpg",
            "Series/Show/Season 1/Show S01E01.mp4",
            "Movies/Alien (1979)/poster.jpg",
            "Movies/Alien (1979)/Alien.mp4",
            "Movies/Other.mp4",
        ]
        .iter()
        .map(|k| ((*k).to_owned(), 10))
        .collect();
        let all = classify_all(&listing, &Fake, &SidecarMap::new(), Options::default());
        let get = |k: &str| all.iter().find(|c| c.key == k).unwrap();
        assert_eq!(
            get("Manga/Berserk/Vol 01.cbz").cover_key.as_deref(),
            Some("Manga/Berserk/cover.jpg")
        );
        assert_eq!(get("Manga/Berserk/Extra.cbz").number, Some(3.0));
        let ep = get("Series/Show/Season 1/Show S01E01.mp4");
        assert_eq!(ep.cover_key.as_deref(), Some("Series/Show/poster.png"));
        assert_eq!(ep.backdrop_key.as_deref(), Some("Series/Show/backdrop.jpg"));
        assert_eq!(
            get("Movies/Alien (1979)/Alien.mp4").cover_key.as_deref(),
            Some("Movies/Alien (1979)/poster.jpg")
        );
        assert_eq!(get("Movies/Other.mp4").cover_key, None);
    }

    #[test]
    fn keys_that_are_not_files_are_ignored() {
        assert!(c("Movies/").ignored);
        assert!(c("Movies/.DS_Store").ignored);
        assert!(c("Movies/Thumbs.db").ignored);
        assert_eq!(c("Movies/.sideroom.json").class, Class::Sidecar);
    }
}

#[cfg(test)]
mod rom_tests {
    use super::*;

    struct NoVideo;
    impl NameParser for NoVideo {
        fn video(&self, _path: &str) -> VideoFacts {
            VideoFacts::default()
        }
    }

    fn c(key: &str) -> Classified {
        classify(key, 10, &NoVideo, &SidecarMap::new(), Options::default())
    }

    #[test]
    fn roms_live_under_emu_roms_only() {
        let nes = c("emu/roms/Test Game.nes");
        assert_eq!(nes.class, Class::Rom);
        assert_eq!(nes.class.kind(), Some(Kind::Games));
        assert_eq!(nes.platform, Some("nes"));
        assert_eq!(nes.core, Some("fceumm"));
        assert_eq!(nes.title, "Test Game");
        let bin = c("emu/roms/disc/game.bin");
        assert_eq!(bin.class, Class::Rom);
        assert_eq!(bin.platform, None);
        // Saves, BIOS files and ROM look-alikes elsewhere are not cataloged as ROMs.
        assert!(c("emu/saves/fceumm/ab/sram.srm").ignored);
        assert!(c("emu/bios/scph1001.bin").ignored);
        assert!(c("emu/roms/readme.txt").ignored);
        assert_ne!(c("Games/x.nes").class, Class::Rom);
        assert_eq!(Class::parse("rom"), Some(Class::Rom));
    }
}
