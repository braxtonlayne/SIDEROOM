//! Library ingest naming parser (REV-UP-007, docs/specs/ingest-naming-conventions.md section 2).
//!
//! Reads a path such as `Series/Vol 02/Chapter 013 - Title/page_001.jpg` or `Series v02 c013 -
//! Title [Group].cbz` into a [`ParsedReaderName`]: series, volume, chapter (decimals allowed
//! for extras), title, release group, page number and file role. Case-insensitive; `_`, `.`
//! and space are interchangeable; `[tags]` carry the release group. AVIF pages are refused
//! (D-169). Pure functions over untrusted names: bounded length, no panics, no I/O.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use utoipa::ToSchema;

const MAX_PATH: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReaderRole {
    /// An image page inside a chapter folder.
    Page,
    /// A `.cbz`/`.zip`/`.cbr`/`.rar`/`.cb7`/`.7z` chapter archive.
    Archive,
    /// `.pdf` or `.epub`.
    Document,
    /// `series.json` at the series root.
    SeriesJson,
    /// `ComicInfo.xml`.
    ComicInfo,
    /// A folder naming a chapter or volume (no file).
    Folder,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ParsedReaderName {
    pub role: ReaderRole,
    pub series: Option<String>,
    pub volume: Option<f64>,
    pub chapter: Option<f64>,
    pub title: Option<String>,
    pub group: Option<String>,
    pub page: Option<u32>,
    /// `_spread` suffix on a page name.
    pub spread: bool,
    pub extension: String,
    pub warnings: Vec<String>,
}

const PAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp", "gif"];
const ARCHIVE_EXT: &[&str] = &["cbz", "zip", "cbr", "rar", "cb7", "7z"];
const DOC_EXT: &[&str] = &["pdf", "epub"];

fn re(p: &str) -> Regex {
    Regex::new(p).expect("static pattern")
}

static TAG: LazyLock<Regex> = LazyLock::new(|| re(r"\[([^\[\]]*)\]|\(([^()]*)\)"));
static VOLUME: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(?:^|[^a-z0-9])(?:volume|vol|v)[\s._-]*(\d{1,3}(?:\.\d)?)(?:[^a-z0-9.]|$)")
});
static CHAPTER: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(?:^|[^a-z0-9])(?:chapter|ch|c)[\s._-]*(\d{1,4}(?:\.\d{1,2})?)(?:[^a-z0-9.]|$)")
});
static TRAILING_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(.*?)\s-\s(\d{1,4}(?:\.\d{1,2})?)(?:\s-\s(.*))?$"));
static PAGE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)^(?:page|p|img|image)?[\s._-]*(\d{1,5})$"));

fn ext_of(name: &str) -> (&str, String) {
    match name.rsplit_once('.') {
        Some((s, e))
            if !s.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            (s, e.to_ascii_lowercase())
        }
        _ => (name, String::new()),
    }
}

fn spaced(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars() {
        let c = if c == '_' { ' ' } else { c };
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

fn trim_sep(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | ',' | ':'))
}

fn num(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn blank(ext: String, role: ReaderRole) -> ParsedReaderName {
    ParsedReaderName {
        role,
        series: None,
        volume: None,
        chapter: None,
        title: None,
        group: None,
        page: None,
        spread: false,
        extension: ext,
        warnings: Vec::new(),
    }
}

/// What one folder or archive name says.
#[derive(Default)]
struct Fields {
    series: Option<String>,
    volume: Option<f64>,
    chapter: Option<f64>,
    title: Option<String>,
    group: Option<String>,
}

fn read_name(raw: &str) -> Fields {
    let mut f = Fields::default();
    // Tags: the last `[Group]` is the release group; other tags (year, quality) are dropped.
    let mut body = String::new();
    let mut last = 0;
    for m in TAG.captures_iter(raw) {
        let whole = m.get(0).expect("whole match");
        body.push_str(&raw[last..whole.start()]);
        body.push(' ');
        last = whole.end();
        if let Some(g) = m.get(1) {
            let g = g.as_str().trim();
            if !g.is_empty() && g.parse::<f64>().is_err() {
                f.group = Some(g.to_owned());
            }
        }
    }
    body.push_str(&raw[last..]);
    let body = spaced(&body);

    let vol = VOLUME.captures(&body);
    let chap = CHAPTER.captures(&body);
    if vol.is_none() && chap.is_none() {
        // `Series - 012` or `Series - 012 - Title`.
        if let Some(c) = TRAILING_NUMBER.captures(&body) {
            f.series = non_empty(trim_sep(&c[1]));
            f.chapter = num(&c[2]);
            f.title = c.get(3).and_then(|t| non_empty(trim_sep(t.as_str())));
        } else {
            f.series = non_empty(trim_sep(&body));
        }
        return f;
    }
    let first = [
        vol.as_ref().map(|c| c.get(0).expect("whole").start()),
        chap.as_ref().map(|c| c.get(0).expect("whole").start()),
    ]
    .into_iter()
    .flatten()
    .min()
    .unwrap_or(0);
    f.series = non_empty(trim_sep(&body[..first]));
    f.volume = vol.as_ref().and_then(|c| num(&c[1]));
    f.chapter = chap.as_ref().and_then(|c| num(&c[1]));
    let end = [vol.as_ref(), chap.as_ref()]
        .into_iter()
        .flatten()
        .map(|c| c.get(1).expect("number").end())
        .max()
        .unwrap_or(0);
    // The title follows the last marker: `c013 - Title`.
    let rest = trim_sep(&body[end.min(body.len())..]);
    f.title = non_empty(rest);
    f
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// Parse a path (`/` or `\` separated, relative to the import root).
pub fn parse_path(path: &str) -> ParsedReaderName {
    let path: String = path.chars().take(MAX_PATH).collect();
    let norm = path.replace('\\', "/");
    let mut parts: Vec<&str> = norm.split('/').filter(|p| !p.is_empty()).collect();
    let Some(file) = parts.pop() else {
        let mut p = blank(String::new(), ReaderRole::Unknown);
        p.warnings.push("empty name".into());
        return p;
    };
    // Folder context: the first plain folder is the series; volume and chapter folders add.
    let mut ctx = Fields::default();
    for d in &parts {
        let f = read_name(d);
        if f.volume.is_some() || f.chapter.is_some() {
            ctx.volume = f.volume.or(ctx.volume);
            ctx.chapter = f.chapter.or(ctx.chapter);
            ctx.title = f.title.or(ctx.title.take());
            ctx.group = f.group.or(ctx.group.take());
            if ctx.series.is_none() {
                ctx.series = f.series;
            }
        } else if ctx.series.is_none() {
            ctx.series = f.series;
            ctx.group = f.group.or(ctx.group.take());
        }
    }

    let lower = file.to_ascii_lowercase();
    if lower == "series.json" {
        let mut p = blank("json".into(), ReaderRole::SeriesJson);
        p.series = ctx.series;
        return p;
    }
    if lower == "comicinfo.xml" {
        let mut p = blank("xml".into(), ReaderRole::ComicInfo);
        p.series = ctx.series;
        p.volume = ctx.volume;
        p.chapter = ctx.chapter;
        return p;
    }
    let (stem, ext) = ext_of(file);
    let mut p = if PAGE_EXT.contains(&ext.as_str()) {
        parse_page(stem, ext, &ctx)
    } else if ARCHIVE_EXT.contains(&ext.as_str()) || DOC_EXT.contains(&ext.as_str()) {
        let role = if ARCHIVE_EXT.contains(&ext.as_str()) {
            ReaderRole::Archive
        } else {
            ReaderRole::Document
        };
        let f = read_name(stem);
        let mut p = blank(ext, role);
        p.series = f.series.or(ctx.series.clone());
        p.volume = f.volume.or(ctx.volume);
        p.chapter = f.chapter.or(ctx.chapter);
        p.title = f.title.or(ctx.title.clone());
        p.group = f.group.or(ctx.group.clone());
        if p.chapter.is_none() && p.volume.is_none() {
            p.warnings.push("no chapter or volume number found".into());
        }
        p
    } else if ext == "avif" {
        let mut p = blank(ext, ReaderRole::Unknown);
        p.warnings
            .push("AVIF pages are not accepted; convert them to JPEG, PNG or WebP first".into());
        p
    } else if ext.is_empty() {
        // A folder name.
        let f = read_name(file);
        let mut p = blank(ext, ReaderRole::Folder);
        p.series = f.series.or(ctx.series.clone());
        p.volume = f.volume.or(ctx.volume);
        p.chapter = f.chapter.or(ctx.chapter);
        p.title = f.title;
        p.group = f.group.or(ctx.group.clone());
        p
    } else {
        let mut p = blank(ext, ReaderRole::Unknown);
        p.warnings.push("not a page, archive or document".into());
        p
    };
    if p.extension == "avif" {
        p.volume = ctx.volume;
        p.chapter = ctx.chapter;
    }
    if p.series.is_none() {
        p.series = ctx.series;
    }
    p
}

fn parse_page(stem: &str, ext: String, ctx: &Fields) -> ParsedReaderName {
    let mut p = blank(ext, ReaderRole::Page);
    p.series = ctx.series.clone();
    p.volume = ctx.volume;
    p.chapter = ctx.chapter;
    p.title = ctx.title.clone();
    p.group = ctx.group.clone();
    let mut s = spaced(stem);
    let l = s.to_ascii_lowercase();
    if let Some(base) = l.strip_suffix(" spread") {
        p.spread = true;
        s = s[..base.len()].to_owned();
    }
    match PAGE.captures(&s) {
        Some(c) => p.page = c[1].parse().ok(),
        None => p
            .warnings
            .push("page name has no number; pages are ordered by natural sort".into()),
    }
    p
}

#[cfg(test)]
mod fixtures;
