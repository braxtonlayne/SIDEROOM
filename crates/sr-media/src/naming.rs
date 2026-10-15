//! Cinema ingest naming parser (REV-UP-007, docs/specs/ingest-naming-conventions.md section 1).
//!
//! Turns a path such as `Cowboy Bebop (1998)/Season 01/Cowboy Bebop - s01e01 - Asteroid
//! Blues.mkv` into a [`ParsedVideoName`]: movie or episode, title, year, season, episode
//! numbers, variant, subtitle language and flags, artwork role. Case-insensitive; `_`, `.`,
//! space and `-` are interchangeable separators; numbers may be zero padded; text in `[ ]`
//! and `( )` is a tag (release group, quality) and never counts as numbering, except a
//! four digit `(year)`. A name that does not match still parses (role `unknown`, a warning);
//! the importer then asks the user for the details.
//!
//! Pure functions over untrusted strings: bounded input length, no panics, no I/O.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use utoipa::ToSchema;

/// Longest path considered; the rest is ignored (names are untrusted).
const MAX_PATH: usize = 1024;

/// What the file is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Video,
    Subtitle,
    Artwork,
    Unknown,
}

/// Movie or episode (for a video or the video a subtitle belongs to).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Media {
    Movie,
    Episode,
    Unknown,
}

/// Which artwork file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkKind {
    Poster,
    Backdrop,
    Logo,
    SeasonPoster,
    EpisodeThumb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ParsedVideoName {
    pub role: Role,
    pub media: Media,
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u32>,
    /// More than one for a multi-episode file (`s01e01-e02`).
    pub episodes: Vec<u32>,
    pub episode_title: Option<String>,
    /// Numbered `Show - 05` without a season (anime absolute order); season is then 1.
    pub absolute: bool,
    /// Season 0 / a `Specials` folder.
    pub special: bool,
    /// Alternate cut of a movie: `Director's Cut`.
    pub variant: Option<String>,
    /// Subtitle language tag as written (`en`, `pt-BR`).
    pub language: Option<String>,
    pub forced: bool,
    pub sdh: bool,
    pub artwork: Option<ArtworkKind>,
    /// Bracket and parenthesis tags and quality words, in order.
    pub tags: Vec<String>,
    pub extension: String,
    pub warnings: Vec<String>,
}

const VIDEO_EXT: &[&str] = &[
    "mkv", "mp4", "m4v", "mov", "avi", "webm", "ts", "mpeg", "mpg", "wmv", "m2ts", "ogv", "flv",
];
const SUB_EXT: &[&str] = &["srt", "ass", "ssa", "vtt", "sub"];
const IMG_EXT: &[&str] = &["jpg", "jpeg", "png", "webp"];

fn re(p: &str) -> Regex {
    Regex::new(p).expect("static pattern")
}

static TAG: LazyLock<Regex> = LazyLock::new(|| re(r"\[([^\[\]]*)\]|\(([^()]*)\)"));
static SXXEXX: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:^|[^a-z0-9])s(\d{1,2})[\s._-]?e(\d{1,4})"));
/// Continuation of an episode number: `-e03` (range), `-03` (range, no spaces), `e02` (next).
static MORE_EPS: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)^(?:\s*-\s*e(\d{1,4})\b|-(\d{1,4})\b|\s*e(\d{1,4})\b)"));
static NXNN: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:^|[^a-z0-9])(\d{1,2})x(\d{1,3})(?:[^a-z0-9]|$)"));
static SEASON_EPISODE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(?:^|[^a-z0-9])season[\s._-]*(\d{1,2})[\s._-]*(?:episode|ep)[\s._-]*(\d{1,4})")
});
static EPISODE_ONLY: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:^|[^a-z0-9])(?:episode|ep|e)[\s._-]*(\d{1,4})(?:[^a-z0-9]|$)"));
static ABSOLUTE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\s-\s(\d{1,4})(?:v\d)?(?:\s|$)"));
static SEASON_DIR: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)^(?:season|series|s)[\s._-]*(\d{1,2})$"));
static YEAR_DIR: LazyLock<Regex> = LazyLock::new(|| re(r"^(.*?)\s*\((\d{4})\)\s*$"));
static SEASON_POSTER: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)^season[\s._-]*(\d{1,2})[\s._-]*poster$"));
static SUB_TAIL_LANG: LazyLock<Regex> =
    LazyLock::new(|| re(r"^[A-Za-z]{2,3}(?:[-_][A-Za-z0-9]{2,4})?$"));
static MOVIE_VARIANT: LazyLock<Regex> = LazyLock::new(|| re(r"^(.*?)\s*\((\d{4})\)\s*-\s*(.+)$"));

const JUNK: &[&str] = &[
    "480p", "576p", "720p", "1080p", "1080i", "2160p", "4k", "uhd", "hd", "sd", "web", "webdl",
    "web-dl", "webrip", "web-rip", "bluray", "blu-ray", "bdrip", "brrip", "dvdrip", "hdtv",
    "hdrip", "x264", "x265", "h264", "h265", "hevc", "avc", "xvid", "aac", "ac3", "eac3", "dts",
    "flac", "10bit", "8bit", "hdr", "hdr10", "remux", "proper", "repack", "multi", "dual", "amzn",
    "nf", "dsnp", "hmax", "atvp", "dd5", "dd2", "ddp5", "5.1", "2.0", "extended",
];

fn is_junk(token: &str) -> bool {
    let t = token.to_lowercase();
    JUNK.contains(&t.as_str())
}

fn ext_of(name: &str) -> (&str, String) {
    if let Some((stem, ext)) = name.rsplit_once('.') {
        let e = ext.to_ascii_lowercase();
        if VIDEO_EXT.contains(&e.as_str())
            || SUB_EXT.contains(&e.as_str())
            || IMG_EXT.contains(&e.as_str())
        {
            return (stem, e);
        }
    }
    (name, String::new())
}

/// Replace `_` and `.` by spaces and collapse white space.
fn spaced(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars() {
        let c = if c == '_' || c == '.' { ' ' } else { c };
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out.trim().to_owned()
}

/// Trim separators and spaces from both ends.
fn trim_sep(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '.' | ',' | ':'))
}

/// Remove `[tag]` and `(tag)` groups; a lone `(YYYY)` becomes the year.
fn strip_tags(s: &str, tags: &mut Vec<String>, year: &mut Option<u16>) -> String {
    let mut out = String::new();
    let mut last = 0;
    for m in TAG.captures_iter(s) {
        let whole = m.get(0).expect("whole match");
        out.push_str(&s[last..whole.start()]);
        out.push(' ');
        last = whole.end();
        let inner = m
            .get(1)
            .or_else(|| m.get(2))
            .map_or("", |g| g.as_str())
            .trim();
        let is_paren = m.get(2).is_some();
        if is_paren
            && inner.len() == 4
            && let Ok(y) = inner.parse::<u16>()
            && (1880..=2100).contains(&y)
            && year.is_none()
        {
            *year = Some(y);
        } else if !inner.is_empty() {
            tags.push(inner.to_owned());
        }
    }
    out.push_str(&s[last..]);
    out
}

/// Drop trailing quality words (`1080p WEB-DL x264`) from `s`, recording them as tags.
fn strip_junk_tail(s: &str, tags: &mut Vec<String>) -> String {
    let words: Vec<&str> = s.split(' ').filter(|w| !w.is_empty()).collect();
    let mut cut = words.len();
    while cut > 0 && is_junk(words[cut - 1]) {
        cut -= 1;
    }
    for w in &words[cut..] {
        tags.push((*w).to_owned());
    }
    words[..cut].join(" ")
}

/// Keep the words before the first quality word; the rest are tags.
fn cut_at_junk(s: &str, tags: &mut Vec<String>) -> String {
    let words: Vec<&str> = s.split(' ').filter(|w| !w.is_empty()).collect();
    let cut = words.iter().position(|w| is_junk(w)).unwrap_or(words.len());
    for w in &words[cut..] {
        tags.push((*w).to_owned());
    }
    words[..cut].join(" ")
}

fn blank(ext: String) -> ParsedVideoName {
    ParsedVideoName {
        role: Role::Unknown,
        media: Media::Unknown,
        title: String::new(),
        year: None,
        season: None,
        episodes: Vec::new(),
        episode_title: None,
        absolute: false,
        special: false,
        variant: None,
        language: None,
        forced: false,
        sdh: false,
        artwork: None,
        tags: Vec::new(),
        extension: ext,
        warnings: Vec::new(),
    }
}

/// Expand the episode numbers that follow the first one; returns the bytes consumed.
fn more_episodes(rest: &str, first: u32, eps: &mut Vec<u32>) -> usize {
    let mut consumed = 0;
    let mut prev = first;
    while let Some(c) = MORE_EPS.captures(&rest[consumed..]) {
        let (n, range) = match (c.get(1), c.get(2), c.get(3)) {
            (Some(g), _, _) | (_, Some(g), _) => (g.as_str().parse::<u32>().unwrap_or(0), true),
            (_, _, Some(g)) => (g.as_str().parse::<u32>().unwrap_or(0), false),
            _ => break,
        };
        // A range only runs upwards and never over a whole season (a year or a title that
        // starts with a number is not an episode).
        if n == 0 || (range && (n <= prev || n - prev > 30)) || (!range && n == prev) {
            break;
        }
        if range {
            eps.extend(prev + 1..=n);
        } else {
            eps.push(n);
        }
        prev = n;
        consumed += c.get(0).expect("whole match").end();
    }
    consumed
}

struct Folders {
    show: Option<String>,
    year: Option<u16>,
    season: Option<u32>,
    special: bool,
}

fn read_folders(dirs: &[&str]) -> Folders {
    let mut f = Folders {
        show: None,
        year: None,
        season: None,
        special: false,
    };
    for d in dirs {
        let d = spaced(d);
        if d.eq_ignore_ascii_case("specials") || d.eq_ignore_ascii_case("special") {
            f.season = Some(0);
            f.special = true;
        } else if let Some(c) = SEASON_DIR.captures(&d) {
            let n = c[1].parse().unwrap_or(0);
            f.season = Some(n);
            f.special = n == 0;
        } else if f.show.is_none() {
            let mut tags = Vec::new();
            let mut year = None;
            let stripped = strip_tags(&d, &mut tags, &mut year);
            let title = trim_sep(&spaced(&stripped)).to_owned();
            if !title.is_empty() {
                f.show = Some(title);
                f.year = year;
            }
        }
    }
    f
}

/// Parse a path (`/` or `\` separated, relative to the import root).
pub fn parse_path(path: &str) -> ParsedVideoName {
    let path: String = path.chars().take(MAX_PATH).collect();
    let norm = path.replace('\\', "/");
    let mut parts: Vec<&str> = norm.split('/').filter(|p| !p.is_empty()).collect();
    let Some(file) = parts.pop() else {
        let mut p = blank(String::new());
        p.warnings.push("empty name".into());
        return p;
    };
    let folders = read_folders(&parts);
    let (stem, ext) = ext_of(file);
    let mut out = blank(ext.clone());

    if IMG_EXT.contains(&ext.as_str()) {
        return parse_artwork(stem, ext, &folders);
    }
    let is_sub = SUB_EXT.contains(&ext.as_str());
    let is_video = VIDEO_EXT.contains(&ext.as_str());
    if !is_sub && !is_video {
        out.warnings
            .push("not a video, subtitle or image file".into());
        out.title = trim_sep(&spaced(stem)).to_owned();
        return out;
    }
    out.role = if is_sub { Role::Subtitle } else { Role::Video };

    let mut stem = stem.to_owned();
    if is_sub {
        stem = take_subtitle_tail(&stem, &mut out);
    }
    let mut tags = Vec::new();
    let mut year = None;
    // A movie variant (`Title (Year) - Director's Cut`) is read before the tags go.
    let variant_split = MOVIE_VARIANT.captures(&spaced(&stem)).map(|c| {
        (
            c[1].to_owned(),
            c[2].parse::<u16>().ok(),
            c[3].trim().to_owned(),
        )
    });
    let body = strip_tags(&stem, &mut tags, &mut year);
    let body = spaced(&body);
    let body = strip_junk_tail(&body, &mut tags);

    match find_episode(&body, &folders) {
        Some(e) => {
            out.media = Media::Episode;
            out.season = e.season.or(folders.season);
            out.episodes = e.episodes;
            out.absolute = e.absolute;
            let title = trim_sep(&body[..e.start]).to_owned();
            out.title = if title.is_empty() {
                folders.show.clone().unwrap_or_default()
            } else {
                title
            };
            let after = cut_at_junk(trim_sep(&body[e.end..]), &mut tags);
            let after = trim_sep(&after).to_owned();
            if !after.is_empty() {
                out.episode_title = Some(after);
            }
            if out.season == Some(0) || folders.special {
                out.special = true;
            }
            if out.season.is_none() {
                out.season = Some(1);
                if !out.absolute {
                    out.warnings
                        .push("no season found, assuming season 1".into());
                }
            }
            if out.title.is_empty() {
                out.warnings.push("no show title".into());
            }
        }
        None => {
            out.media = Media::Movie;
            if let Some((t, y, v)) = variant_split {
                let t = trim_sep(&spaced(&t)).to_owned();
                if !t.is_empty() {
                    out.title = t;
                    year = year.or(y);
                    let v = spaced(&strip_tags(&v, &mut Vec::new(), &mut None));
                    let v = trim_sep(&v).to_owned();
                    if !v.is_empty() {
                        out.variant = Some(v);
                    }
                }
            }
            if out.title.is_empty() {
                out.title = trim_sep(&body).to_owned();
            }
            if out.title.is_empty() {
                out.title = folders.show.clone().unwrap_or_default();
            }
            if year.is_none() {
                year = folders.year;
            }
            if year.is_none()
                && let Some((t, y)) = release_cut(&out.title)
            {
                out.title = t;
                year = Some(y);
            }
            if year.is_none() {
                // `Title 1988`: a trailing year in the title.
                if let Some((t, y)) = trailing_year(&out.title) {
                    out.title = t;
                    year = Some(y);
                } else {
                    out.warnings.push("no year in the name".into());
                }
            }
            if out.title.is_empty() {
                out.warnings.push("no title".into());
                out.media = Media::Unknown;
            }
        }
    }
    out.year = year.or(folders.year);
    out.tags = tags;
    out
}

/// Scene style `Title 2001 1080p BluRay x264-GRP`: the year ends the title when quality
/// words follow it.
fn release_cut(title: &str) -> Option<(String, u16)> {
    let words: Vec<&str> = title.split(' ').collect();
    for i in 1..words.len().saturating_sub(1) {
        if words[i].len() == 4
            && let Ok(y) = words[i].parse::<u16>()
            && (1900..=2100).contains(&y)
            && is_junk(words[i + 1])
        {
            return Some((words[..i].join(" "), y));
        }
    }
    None
}

fn trailing_year(title: &str) -> Option<(String, u16)> {
    let (head, last) = title.rsplit_once(' ')?;
    if last.len() == 4
        && let Ok(y) = last.parse::<u16>()
        && (1900..=2100).contains(&y)
        && !head.trim().is_empty()
    {
        return Some((head.trim().to_owned(), y));
    }
    None
}

struct Found {
    season: Option<u32>,
    episodes: Vec<u32>,
    absolute: bool,
    start: usize,
    end: usize,
}

fn find_episode(body: &str, folders: &Folders) -> Option<Found> {
    if let Some(c) = SXXEXX.captures(body) {
        let whole = c.get(0)?;
        let first: u32 = c[2].parse().ok()?;
        let mut eps = vec![first];
        let used = more_episodes(&body[whole.end()..], first, &mut eps);
        return Some(Found {
            season: c[1].parse().ok(),
            episodes: eps,
            absolute: false,
            start: whole.start(),
            end: whole.end() + used,
        });
    }
    if let Some(c) = SEASON_EPISODE.captures(body) {
        let whole = c.get(0)?;
        let first: u32 = c[2].parse().ok()?;
        let mut eps = vec![first];
        let used = more_episodes(&body[whole.end()..], first, &mut eps);
        return Some(Found {
            season: c[1].parse().ok(),
            episodes: eps,
            absolute: false,
            start: whole.start(),
            end: whole.end() + used,
        });
    }
    if let Some(c) = NXNN.captures(body) {
        let m1 = c.get(1)?;
        let m2 = c.get(2)?;
        let first: u32 = m2.as_str().parse().ok()?;
        let mut eps = vec![first];
        let used = more_episodes(&body[m2.end()..], first, &mut eps);
        return Some(Found {
            season: m1.as_str().parse().ok(),
            episodes: eps,
            absolute: false,
            start: c.get(0)?.start(),
            end: m2.end() + used,
        });
    }
    // `E05` / `Episode 5`: the season comes from the folder.
    if let Some(c) = EPISODE_ONLY.captures(body) {
        let whole = c.get(0)?;
        let num = c.get(1)?;
        let first: u32 = num.as_str().parse().ok()?;
        let mut eps = vec![first];
        let used = more_episodes(&body[num.end()..], first, &mut eps);
        return Some(Found {
            season: folders.season,
            episodes: eps,
            absolute: false,
            start: whole.start(),
            end: num.end() + used,
        });
    }
    if let Some(c) = ABSOLUTE.captures(body) {
        let whole = c.get(0)?;
        let n: u32 = c[1].parse().ok()?;
        // A year after a dash is a title part, not an episode.
        if (1900..=2100).contains(&n) {
            return None;
        }
        return Some(Found {
            season: folders.season.or(Some(1)),
            episodes: vec![n],
            absolute: folders.season.is_none(),
            start: whole.start(),
            end: whole.end(),
        });
    }
    // A bare number in a season folder (`Season 01/05.mkv`, `Season 01/05 - Title.mkv`).
    if folders.season.is_some() {
        let t = body.trim_start();
        let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() && digits.len() <= 4 {
            let n: u32 = digits.parse().ok()?;
            let start = body.len() - t.len();
            return Some(Found {
                season: folders.season,
                episodes: vec![n],
                absolute: false,
                start,
                end: start + digits.len(),
            });
        }
    }
    None
}

/// Pop `.lang`, `.forced`, `.sdh` from the end of a subtitle stem, in any order.
fn take_subtitle_tail(stem: &str, out: &mut ParsedVideoName) -> String {
    let mut parts: Vec<&str> = stem.split('.').collect();
    while parts.len() > 1 {
        let last = parts[parts.len() - 1];
        let l = last.to_ascii_lowercase();
        if matches!(l.as_str(), "forced" | "force") {
            out.forced = true;
        } else if matches!(l.as_str(), "sdh" | "hi" | "cc") {
            out.sdh = true;
        } else if matches!(l.as_str(), "default" | "full") {
            // Flags that carry no meaning here.
        } else if out.language.is_none()
            && SUB_TAIL_LANG.is_match(last)
            && !last.chars().all(|c| c.is_ascii_digit())
        {
            out.language = Some(last.to_owned());
        } else {
            break;
        }
        parts.pop();
    }
    parts.join(".")
}

fn parse_artwork(stem: &str, ext: String, folders: &Folders) -> ParsedVideoName {
    let mut out = blank(ext);
    out.role = Role::Artwork;
    let name = spaced(stem);
    let lower = name.to_lowercase();
    if let Some(c) = SEASON_POSTER.captures(&name) {
        out.artwork = Some(ArtworkKind::SeasonPoster);
        out.season = c[1].parse().ok();
        out.special = out.season == Some(0);
    } else if let Some(base) = lower
        .strip_suffix(" thumb")
        .or_else(|| lower.strip_suffix("-thumb"))
    {
        out.artwork = Some(ArtworkKind::EpisodeThumb);
        // The thumb names the episode file it belongs to.
        let inner = parse_path(&format!("{}.mkv", &name[..base.len()]));
        out.title = inner.title;
        out.season = inner.season;
        out.episodes = inner.episodes;
        out.year = inner.year;
        out.media = Media::Episode;
    } else {
        out.artwork = match lower.as_str() {
            "poster" | "folder" | "cover" => Some(ArtworkKind::Poster),
            "backdrop" | "fanart" | "background" => Some(ArtworkKind::Backdrop),
            "logo" | "clearlogo" => Some(ArtworkKind::Logo),
            _ => None,
        };
    }
    if out.artwork.is_none() {
        out.role = Role::Unknown;
        out.warnings.push("unrecognised image name".into());
        out.title = trim_sep(&name).to_owned();
    }
    if out.title.is_empty() {
        out.title = folders.show.clone().unwrap_or_default();
    }
    out.year = out.year.or(folders.year);
    if out.season.is_none() {
        out.season = folders.season;
    }
    out
}

/// The folder a title belongs to: `Show (Year)` yields the show and year.
pub fn parse_title_folder(name: &str) -> (String, Option<u16>) {
    let s = spaced(name);
    match YEAR_DIR.captures(&s) {
        Some(c) => (trim_sep(&c[1]).to_owned(), c[2].parse().ok()),
        None => (trim_sep(&s).to_owned(), None),
    }
}

#[cfg(test)]
mod fixtures;
