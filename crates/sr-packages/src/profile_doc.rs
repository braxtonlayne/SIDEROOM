//! The profile document (EXT-PRF-001, docs/10): what a person's customised profile page is. Pure data on
//! a 12-column grid: a list of blocks, each with a position and a small typed config, and style choices
//! taken from closed lists (approved fonts, cursors, overlays, borders, patterns) plus a palette. It never
//! holds markup, CSS or script (D-063), and every link it carries is `https`. The same validator checks
//! a person's own document and the template inside a `profile_layout` package (where block configs must
//! be empty, because a layout is shared and carries nobody's friends, links or text).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

pub const GRID_COLUMNS: u32 = 12;
pub const MAX_ROWS: u32 = 60;
pub const MAX_BLOCKS: usize = 16;
pub const MAX_DOC_BYTES: usize = 32 * 1024;
pub const DOC_VERSION: u32 = 1;

/// Approved fonts (all four ship with the app, so no font file is ever fetched from elsewhere).
pub const FONTS: &[&str] = &["inter", "newsreader", "hanken-grotesk", "plex-mono"];
/// Cursor sets: CSS keywords only, no images.
pub const CURSORS: &[&str] = &["default", "pointer", "crosshair", "cell", "help"];
/// Decorative overlays; all are CSS effects and respect reduced motion.
pub const OVERLAYS: &[&str] = &["none", "sparkle", "stars", "hearts", "snow"];
pub const BORDERS: &[&str] = &["none", "solid", "dashed", "dotted", "double", "groove"];
pub const PATTERNS: &[&str] = &["none", "dots", "grid", "stripes", "checks", "waves"];
/// Hosts a Song block may point to (the client embeds them only after a click).
pub const SONG_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "youtu.be",
    "soundcloud.com",
    "bandcamp.com",
    "open.spotify.com",
];
/// Where the block types may appear at most.
const SINGLETONS: &[&str] = &["header", "guestbook", "top_friends", "now_watching"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ProfileBlockKind)]
pub enum BlockKind {
    Header,
    About,
    TopFriends,
    NowWatching,
    Favorites,
    Gallery,
    Guestbook,
    Song,
    Custom,
}

impl BlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::About => "about",
            Self::TopFriends => "top_friends",
            Self::NowWatching => "now_watching",
            Self::Favorites => "favorites",
            Self::Gallery => "gallery",
            Self::Guestbook => "guestbook",
            Self::Song => "song",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ProfilePalette)]
pub struct Palette {
    /// Page background, `#rrggbb`.
    pub background: String,
    /// Block background.
    pub panel: String,
    /// Text colour.
    pub text: String,
    /// Accent (links, borders, buttons).
    pub accent: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            background: "#1e1f22".to_owned(),
            panel: "#2b2d31".to_owned(),
            text: "#f2f3f5".to_owned(),
            accent: "#5865f2".to_owned(),
        }
    }
}

/// The page background (K-17, D-260): a pattern, my profile banner, or a picture I uploaded. It replaces the
/// older `pattern` plus `banner_background` pair, which is still read when this is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ProfileBackground)]
pub struct Background {
    /// `pattern`, `banner` or `image`.
    pub kind: String,
    /// One of [`PATTERNS`]; drawn over the banner or the picture too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// The id of one of my uploaded background pictures (`kind` = `image`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
}

/// Which of my uploaded pictures a document may name (K-17): the ids of my `background` and `gallery` rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaIds {
    pub background: BTreeSet<String>,
    pub gallery: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ProfileStyle)]
pub struct Style {
    #[serde(default)]
    pub palette: Palette,
    /// One of [`PATTERNS`] (the older form; see [`Style::background`]).
    #[serde(default = "none")]
    pub pattern: String,
    /// Use my profile banner as the page background (the older form).
    #[serde(default)]
    pub banner_background: bool,
    /// The page background; wins over `pattern` and `banner_background` when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    /// One of [`FONTS`].
    #[serde(default = "inter")]
    pub font: String,
    /// One of [`CURSORS`].
    #[serde(default = "default_cursor")]
    pub cursor: String,
    /// One of [`OVERLAYS`].
    #[serde(default = "none")]
    pub overlay: String,
    /// One of [`BORDERS`].
    #[serde(default = "none")]
    pub border: String,
}

fn none() -> String {
    "none".to_owned()
}
fn inter() -> String {
    "inter".to_owned()
}
fn default_cursor() -> String {
    "default".to_owned()
}

impl Default for Style {
    fn default() -> Self {
        Self {
            palette: Palette::default(),
            pattern: none(),
            banner_background: false,
            background: None,
            font: inter(),
            cursor: default_cursor(),
            overlay: none(),
            border: none(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ProfileBlock)]
pub struct Block {
    /// Unique within the document: 1 to 24 characters of `a-z`, `0-9` and `-`.
    pub id: String,
    #[serde(rename = "type")]
    pub kind: BlockKind,
    /// Column of the left edge, 0 to 11.
    pub x: u32,
    /// Row of the top edge.
    pub y: u32,
    /// Width in columns.
    pub w: u32,
    /// Height in rows.
    pub h: u32,
    /// Type-specific settings (see the module documentation).
    #[serde(default)]
    #[schema(value_type = Object)]
    pub config: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileDoc {
    /// Always 1.
    #[serde(default = "version")]
    pub version: u32,
    /// Slug of the approved layout package this document started from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub blocks: Vec<Block>,
}

fn version() -> u32 {
    DOC_VERSION
}

impl Default for ProfileDoc {
    fn default() -> Self {
        Self {
            version: DOC_VERSION,
            layout: None,
            style: Style::default(),
            blocks: Vec::new(),
        }
    }
}

/// One problem with a document: `path` is a JSON-pointer-like location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = ProfileDocProblem)]
pub struct DocProblem {
    pub path: String,
    pub message: String,
}

fn problem(out: &mut Vec<DocProblem>, path: impl Into<String>, message: impl Into<String>) {
    out.push(DocProblem {
        path: path.into(),
        message: message.into(),
    });
}

fn hex(s: &str) -> Option<(u8, u8, u8)> {
    let b = s.as_bytes();
    if b.len() != 7 || b[0] != b'#' || !b[1..].iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some((p(1)?, p(3)?, p(5)?))
}

fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let lin = |c: u8| {
        let v = f64::from(c) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio of two `#rrggbb` colours, or `None` when either is not one.
pub fn contrast(a: &str, b: &str) -> Option<f64> {
    let (la, lb) = (luminance(hex(a)?), luminance(hex(b)?));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    Some((hi + 0.05) / (lo + 0.05))
}

fn text_len(s: &str) -> usize {
    s.chars().count()
}

fn check_text(out: &mut Vec<DocProblem>, path: &str, s: &str, max: usize) {
    if text_len(s) > max {
        problem(out, path, format!("must be at most {max} characters long"));
    }
    if s.chars().any(|c| c.is_control() && c != '\n') {
        problem(out, path, "must not contain control characters");
    }
}

/// An `https` URL without credentials, at most 512 characters.
fn https_url(s: &str) -> Option<url::Url> {
    if s.len() > 512 {
        return None;
    }
    let u = url::Url::parse(s).ok()?;
    (u.scheme() == "https"
        && u.host_str().is_some()
        && u.username().is_empty()
        && u.password().is_none())
    .then_some(u)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderCfg {
    #[serde(default)]
    tagline: Option<String>,
    #[serde(default = "yes")]
    show_banner: bool,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AboutCfg {
    #[serde(default)]
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TopFriendsCfg {
    #[serde(default)]
    user_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NowWatchingCfg {
    /// `auto` shows what my presence says; `manual` shows the lines below.
    #[serde(default = "auto")]
    mode: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    subtitle: Option<String>,
}
fn auto() -> String {
    "auto".to_owned()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FavoritesCfg {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    items: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GalleryItem {
    /// An address on the web (loaded through the image proxy, D-259) ...
    #[serde(default)]
    url: Option<String>,
    /// ... or the id of one of my uploaded gallery pictures (K-17).
    #[serde(default)]
    image_id: Option<String>,
    #[serde(default)]
    caption: Option<String>,
    /// Filled in by the server on a served page; never accepted.
    #[serde(default)]
    proxy_url: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GalleryCfg {
    #[serde(default)]
    items: Vec<GalleryItem>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuestbookCfg {
    #[serde(default)]
    title: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SongCfg {
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    url: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomLink {
    label: String,
    url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomCfg {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    links: Vec<CustomLink>,
}

fn parse<T: for<'de> Deserialize<'de>>(
    out: &mut Vec<DocProblem>,
    path: &str,
    cfg: &Map<String, Value>,
) -> Option<T> {
    match serde_json::from_value::<T>(Value::Object(cfg.clone())) {
        Ok(v) => Some(v),
        Err(e) => {
            problem(out, path, e.to_string());
            None
        }
    }
}

fn media_id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 20
        && id.bytes().all(|b| b.is_ascii_digit())
        && !id.starts_with('0')
}

fn block_config(out: &mut Vec<DocProblem>, i: usize, b: &Block, media: Option<&MediaIds>) {
    let path = format!("blocks[{i}].config");
    let p = |f: &str| format!("{path}.{f}");
    match b.kind {
        BlockKind::Header => {
            if let Some(c) = parse::<HeaderCfg>(out, &path, &b.config) {
                if let Some(t) = &c.tagline {
                    check_text(out, &p("tagline"), t, 80);
                }
                let _ = c.show_banner;
            }
        }
        BlockKind::About => {
            if let Some(c) = parse::<AboutCfg>(out, &path, &b.config) {
                check_text(out, &p("text"), &c.text, 1500);
            }
        }
        BlockKind::TopFriends => {
            if let Some(c) = parse::<TopFriendsCfg>(out, &path, &b.config) {
                if c.user_ids.len() > 8 {
                    problem(out, p("user_ids"), "at most 8 friends");
                }
                let mut seen = BTreeSet::new();
                for id in &c.user_ids {
                    if id.is_empty()
                        || id.len() > 20
                        || !id.bytes().all(|b| b.is_ascii_digit())
                        || id.starts_with('0')
                    {
                        problem(out, p("user_ids"), "must be user ids");
                    } else if !seen.insert(id.clone()) {
                        problem(out, p("user_ids"), "lists a friend twice");
                    }
                }
            }
        }
        BlockKind::NowWatching => {
            if let Some(c) = parse::<NowWatchingCfg>(out, &path, &b.config) {
                if !matches!(c.mode.as_str(), "auto" | "manual") {
                    problem(out, p("mode"), "must be auto or manual");
                }
                for (f, v) in [("title", &c.title), ("subtitle", &c.subtitle)] {
                    if let Some(v) = v {
                        check_text(out, &p(f), v, 80);
                    }
                }
            }
        }
        BlockKind::Favorites => {
            if let Some(c) = parse::<FavoritesCfg>(out, &path, &b.config) {
                if let Some(t) = &c.title {
                    check_text(out, &p("title"), t, 40);
                }
                if c.items.len() > 10 {
                    problem(out, p("items"), "at most 10 items");
                }
                for it in &c.items {
                    check_text(out, &p("items"), it, 60);
                }
            }
        }
        BlockKind::Gallery => {
            if let Some(c) = parse::<GalleryCfg>(out, &path, &b.config) {
                if c.items.len() > 12 {
                    problem(out, p("items"), "at most 12 pictures");
                }
                for it in &c.items {
                    if it.proxy_url.is_some() {
                        problem(out, p("items"), "proxy_url is set by the server");
                    }
                    match (&it.url, &it.image_id) {
                        (Some(u), None) => {
                            if https_url(u).is_none() {
                                problem(out, p("items"), "each picture needs an https address");
                            }
                        }
                        (None, Some(id)) => {
                            if !media_id_ok(id) {
                                problem(out, p("items"), "image_id must be a picture id");
                            } else if media.is_some_and(|m| !m.gallery.contains(id)) {
                                problem(out, p("items"), "image_id is not one of your pictures");
                            }
                        }
                        _ => problem(
                            out,
                            p("items"),
                            "each picture has an https address or an image_id, not both",
                        ),
                    }
                    if let Some(cap) = &it.caption {
                        check_text(out, &p("items"), cap, 80);
                    }
                }
            }
        }
        BlockKind::Guestbook => {
            if let Some(c) = parse::<GuestbookCfg>(out, &path, &b.config)
                && let Some(t) = &c.title
            {
                check_text(out, &p("title"), t, 40);
            }
        }
        BlockKind::Song => {
            if let Some(c) = parse::<SongCfg>(out, &path, &b.config) {
                check_text(out, &p("title"), &c.title, 80);
                if let Some(a) = &c.artist {
                    check_text(out, &p("artist"), a, 80);
                }
                if let Some(u) = &c.url {
                    match https_url(u) {
                        Some(url)
                            if url.host_str().is_some_and(|h| {
                                SONG_HOSTS.contains(&h) || h.ends_with(".bandcamp.com")
                            }) => {}
                        _ => problem(
                            out,
                            p("url"),
                            "must be an https link to YouTube, SoundCloud, Bandcamp or Spotify",
                        ),
                    }
                }
            }
        }
        BlockKind::Custom => {
            if let Some(c) = parse::<CustomCfg>(out, &path, &b.config) {
                if let Some(t) = &c.title {
                    check_text(out, &p("title"), t, 40);
                }
                check_text(out, &p("text"), &c.text, 1000);
                if c.links.len() > 6 {
                    problem(out, p("links"), "at most 6 links");
                }
                for l in &c.links {
                    check_text(out, &p("links"), &l.label, 40);
                    if https_url(&l.url).is_none() {
                        problem(out, p("links"), "each link needs an https address");
                    }
                }
            }
        }
    }
}

fn one_of(out: &mut Vec<DocProblem>, path: &str, v: &str, list: &[&str]) {
    if !list.contains(&v) {
        problem(out, path, format!("must be one of: {}", list.join(", ")));
    }
}

fn slug_ok(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Everything wrong with `doc`, empty when it is acceptable. `template` is the mode for a layout package.
pub fn problems(doc: &ProfileDoc, template: bool) -> Vec<DocProblem> {
    problems_with(doc, template, None)
}

/// [`problems`] that also checks the pictures a document names against `media`, the person's own uploads
/// (K-17): any other id is refused. `None` checks only that the ids look like ids.
pub fn problems_with(
    doc: &ProfileDoc,
    template: bool,
    media: Option<&MediaIds>,
) -> Vec<DocProblem> {
    let mut out = Vec::new();
    if doc.version != DOC_VERSION {
        problem(&mut out, "version", "must be 1");
    }
    if let Some(l) = &doc.layout
        && !slug_ok(l, 48)
    {
        problem(&mut out, "layout", "is not a layout slug");
    }
    let s = &doc.style;
    for (n, v) in [
        ("background", &s.palette.background),
        ("panel", &s.palette.panel),
        ("text", &s.palette.text),
        ("accent", &s.palette.accent),
    ] {
        if hex(v).is_none() {
            problem(&mut out, format!("style.palette.{n}"), "must be #rrggbb");
        }
    }
    for (n, other) in [
        ("background", &s.palette.background),
        ("panel", &s.palette.panel),
    ] {
        if let Some(c) = contrast(&s.palette.text, other)
            && c < 4.5
        {
            problem(
                &mut out,
                "style.palette.text",
                format!("text on the {n} colour has contrast {c:.1}:1; at least 4.5:1 is needed"),
            );
        }
    }
    one_of(&mut out, "style.pattern", &s.pattern, PATTERNS);
    if let Some(bg) = &s.background {
        if let Some(p) = &bg.pattern {
            one_of(&mut out, "style.background.pattern", p, PATTERNS);
        }
        match (bg.kind.as_str(), &bg.image_id) {
            ("pattern" | "banner", None) => {}
            ("image", Some(id)) => {
                if template {
                    problem(
                        &mut out,
                        "style.background.image_id",
                        "a layout carries no pictures",
                    );
                } else if !media_id_ok(id) {
                    problem(
                        &mut out,
                        "style.background.image_id",
                        "must be a picture id",
                    );
                } else if media.is_some_and(|m| !m.background.contains(id)) {
                    problem(
                        &mut out,
                        "style.background.image_id",
                        "is not one of your pictures",
                    );
                }
            }
            ("image", None) => problem(
                &mut out,
                "style.background.image_id",
                "a picture background needs an image_id",
            ),
            ("pattern" | "banner", Some(_)) => problem(
                &mut out,
                "style.background.image_id",
                "only a picture background has an image_id",
            ),
            _ => problem(
                &mut out,
                "style.background.kind",
                "must be pattern, banner or image",
            ),
        }
    }
    one_of(&mut out, "style.font", &s.font, FONTS);
    one_of(&mut out, "style.cursor", &s.cursor, CURSORS);
    one_of(&mut out, "style.overlay", &s.overlay, OVERLAYS);
    one_of(&mut out, "style.border", &s.border, BORDERS);

    if doc.blocks.len() > MAX_BLOCKS {
        problem(&mut out, "blocks", format!("at most {MAX_BLOCKS} blocks"));
    }
    let mut ids = BTreeSet::new();
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for (i, b) in doc.blocks.iter().enumerate() {
        let at = |f: &str| format!("blocks[{i}].{f}");
        if !slug_ok(&b.id, 24) {
            problem(
                &mut out,
                at("id"),
                "must be 1 to 24 characters of a-z, 0-9 and -",
            );
        } else if !ids.insert(b.id.as_str()) {
            problem(&mut out, at("id"), "is used twice");
        }
        *counts.entry(b.kind.as_str()).or_default() += 1;
        if b.w == 0 || b.h == 0 || b.w > GRID_COLUMNS || b.h > 24 {
            problem(
                &mut out,
                at("w"),
                "size must be 1 to 12 columns and 1 to 24 rows",
            );
        } else if b.x + b.w > GRID_COLUMNS || b.y + b.h > MAX_ROWS {
            problem(&mut out, at("x"), "does not fit the 12 column grid");
        }
        if template {
            if !b.config.is_empty() {
                problem(&mut out, at("config"), "a layout carries no content");
            }
        } else {
            block_config(&mut out, i, b, media);
        }
    }
    for (kind, n) in &counts {
        if SINGLETONS.contains(kind) && *n > 1 {
            problem(
                &mut out,
                "blocks",
                format!("only one {kind} block is allowed"),
            );
        }
        if *kind == "song" && *n > 2 {
            problem(&mut out, "blocks", "at most two song blocks");
        }
    }
    for (i, a) in doc.blocks.iter().enumerate() {
        for (j, b) in doc.blocks.iter().enumerate().skip(i + 1) {
            let apart =
                a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
            if !apart {
                problem(
                    &mut out,
                    format!("blocks[{j}]"),
                    format!("overlaps blocks[{i}]"),
                );
            }
        }
    }
    if serde_json::to_vec(doc).map_or(true, |b| b.len() > MAX_DOC_BYTES) {
        problem(&mut out, "", "the document is larger than 32 KiB");
    }
    out
}

/// Parse and check a JSON value. `Err` carries every problem, or the single parse problem.
pub fn parse_doc(value: &Value, template: bool) -> Result<ProfileDoc, Vec<DocProblem>> {
    parse_doc_with(value, template, None)
}

/// [`parse_doc`] that checks the pictures against the person's own uploads (see [`problems_with`]).
pub fn parse_doc_with(
    value: &Value,
    template: bool,
    media: Option<&MediaIds>,
) -> Result<ProfileDoc, Vec<DocProblem>> {
    let doc: ProfileDoc = serde_json::from_value(value.clone()).map_err(|e| {
        vec![DocProblem {
            path: String::new(),
            message: e.to_string(),
        }]
    })?;
    let p = problems_with(&doc, template, media);
    if p.is_empty() { Ok(doc) } else { Err(p) }
}

/// The picture ids a document refers to, as `(background, gallery)`.
pub fn media_refs(doc: &ProfileDoc) -> (BTreeSet<String>, BTreeSet<String>) {
    let background = doc
        .style
        .background
        .iter()
        .filter_map(|b| b.image_id.clone())
        .collect();
    let gallery = doc
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Gallery)
        .filter_map(|b| b.config.get("items").and_then(Value::as_array))
        .flatten()
        .filter_map(|i| i.get("image_id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    (background, gallery)
}

/// The user ids a document names in its Top friends block (to check they are friends).
pub fn top_friend_ids(doc: &ProfileDoc) -> Vec<i64> {
    doc.blocks
        .iter()
        .filter(|b| b.kind == BlockKind::TopFriends)
        .filter_map(|b| b.config.get("user_ids").and_then(Value::as_array))
        .flatten()
        .filter_map(|v| v.as_str()?.parse::<i64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn block(id: &str, kind: &str, x: u32, y: u32, w: u32, h: u32, config: Value) -> Value {
        json!({"id": id, "type": kind, "x": x, "y": y, "w": w, "h": h, "config": config})
    }

    #[test]
    fn the_empty_document_and_a_typical_one_pass() {
        assert!(parse_doc(&json!({}), false).is_ok());
        let doc = json!({
            "version": 1,
            "style": {"font": "newsreader", "overlay": "sparkle", "border": "double", "pattern": "dots"},
            "blocks": [
                block("head", "header", 0, 0, 12, 3, json!({"tagline": "hi"})),
                block("about", "about", 0, 3, 6, 4, json!({"text": "Hello"})),
                block("fr", "top_friends", 6, 3, 6, 4, json!({"user_ids": ["123", "456"]})),
                block("song", "song", 0, 7, 6, 3, json!({"title": "x", "url": "https://youtu.be/abc"})),
                block("gb", "guestbook", 6, 7, 6, 5, json!({})),
            ]
        });
        let d = parse_doc(&doc, false).unwrap();
        assert_eq!(top_friend_ids(&d), vec![123, 456]);
    }

    #[test]
    fn unknown_fields_values_and_overlap_are_refused() {
        let bad = |v: Value| parse_doc(&v, false).unwrap_err();
        assert!(!bad(json!({"style": {"font": "comic-sans"}})).is_empty());
        assert!(!bad(json!({"style": {"cursor": "url(x)"}})).is_empty());
        assert!(!bad(json!({"css": "body{}"})).is_empty());
        let overlap = json!({"blocks": [
            block("a", "about", 0, 0, 6, 4, json!({})),
            block("b", "custom", 5, 3, 6, 4, json!({})),
        ]});
        assert!(bad(overlap).iter().any(|p| p.message.contains("overlaps")));
        let outside = json!({"blocks": [block("a", "about", 8, 0, 6, 4, json!({}))]});
        assert!(bad(outside).iter().any(|p| p.message.contains("grid")));
        let two = json!({"blocks": [
            block("a", "guestbook", 0, 0, 6, 4, json!({})),
            block("b", "guestbook", 6, 0, 6, 4, json!({})),
        ]});
        assert!(bad(two).iter().any(|p| p.message.contains("only one")));
    }

    #[test]
    fn links_must_be_https_and_songs_allow_listed() {
        let song = |u: &str| json!({"blocks": [block("s", "song", 0, 0, 6, 3, json!({"title": "t", "url": u}))]});
        assert!(parse_doc(&song("https://soundcloud.com/a/b"), false).is_ok());
        assert!(parse_doc(&song("https://artist.bandcamp.com/track/x"), false).is_ok());
        assert!(parse_doc(&song("http://youtu.be/x"), false).is_err());
        assert!(parse_doc(&song("https://evil.example/x.mp3"), false).is_err());
        assert!(parse_doc(&song("https://user:pw@youtu.be/x"), false).is_err());
        let custom = json!({"blocks": [block("c", "custom", 0, 0, 6, 3,
            json!({"links": [{"label": "x", "url": "javascript:alert(1)"}]}))]});
        assert!(parse_doc(&custom, false).is_err());
        let gallery = json!({"blocks": [block("g", "gallery", 0, 0, 6, 3,
            json!({"items": [{"url": "https://example.org/a.png"}]}))]});
        assert!(parse_doc(&gallery, false).is_ok());
    }

    #[test]
    fn unreadable_palettes_are_refused() {
        let doc = json!({"style": {"palette": {"background": "#ffffff", "panel": "#fafafa", "text": "#eeeeee", "accent": "#ff0000"}}});
        let p = parse_doc(&doc, false).unwrap_err();
        assert!(p.iter().any(|p| p.message.contains("contrast")));
        assert!((contrast("#000000", "#ffffff").unwrap() - 21.0).abs() < 0.01);
        assert!(parse_doc(&json!({"style": {"palette": {"background": "red", "panel": "#000000", "text": "#ffffff", "accent": "#ff0000"}}}), false).is_err());
    }

    #[test]
    fn a_layout_template_carries_no_content() {
        let ok = json!({"blocks": [block("a", "about", 0, 0, 6, 4, json!({}))]});
        assert!(parse_doc(&ok, true).is_ok());
        let full = json!({"blocks": [block("a", "about", 0, 0, 6, 4, json!({"text": "me"}))]});
        assert!(parse_doc(&full, true).is_err());
    }

    #[test]
    fn counts_and_sizes_are_limited() {
        let many: Vec<Value> = (0..17)
            .map(|i| block(&format!("c{i}"), "custom", 0, i * 3, 4, 2, json!({})))
            .collect();
        assert!(parse_doc(&json!({"blocks": many}), false).is_err());
        let long = "x".repeat(1501);
        assert!(
            parse_doc(
                &json!({"blocks": [block("a", "about", 0, 0, 6, 4, json!({"text": long}))]}),
                false
            )
            .is_err()
        );
        let friends: Vec<String> = (1..10).map(|i| i.to_string()).collect();
        assert!(parse_doc(&json!({"blocks": [block("f", "top_friends", 0, 0, 6, 4, json!({"user_ids": friends}))]}), false).is_err());
    }

    #[test]
    fn pictures_must_be_the_persons_own() {
        let mine = MediaIds {
            background: BTreeSet::from(["11".to_owned()]),
            gallery: BTreeSet::from(["21".to_owned(), "22".to_owned()]),
        };
        let doc = |bg: &str, img: &str| {
            json!({
                "style": {"background": {"kind": "image", "image_id": bg, "pattern": "dots"}},
                "blocks": [block("g", "gallery", 0, 0, 6, 3, json!({"items": [
                    {"image_id": img, "caption": "mine"}, {"url": "https://img.example/a.png"}]}))]
            })
        };
        let d = parse_doc_with(&doc("11", "21"), false, Some(&mine)).unwrap();
        assert_eq!(
            media_refs(&d),
            (
                BTreeSet::from(["11".to_owned()]),
                BTreeSet::from(["21".to_owned()])
            )
        );
        // Somebody else's id, or one of mine used in the wrong place, is refused.
        let foreign = parse_doc_with(&doc("11", "99"), false, Some(&mine)).unwrap_err();
        assert!(
            foreign
                .iter()
                .any(|p| p.message.contains("not one of your pictures"))
        );
        let swapped = parse_doc_with(&doc("21", "21"), false, Some(&mine)).unwrap_err();
        assert!(
            swapped
                .iter()
                .any(|p| p.path == "style.background.image_id")
        );
        // Without a set only the shape is checked; a layout template carries no pictures.
        assert!(parse_doc(&doc("11", "99"), false).is_ok());
        assert!(parse_doc(&doc("11", "21"), true).is_err());
        assert!(parse_doc(&doc("abc", "21"), false).is_err());
    }

    #[test]
    fn gallery_items_name_one_source_and_the_old_background_shape_is_still_read() {
        let item = |v: Value| json!({"blocks": [block("g", "gallery", 0, 0, 6, 3, json!({"items": [v]}))]});
        assert!(parse_doc(&item(json!({"url": "https://a.example/x.png"})), false).is_ok());
        assert!(parse_doc(&item(json!({"image_id": "5"})), false).is_ok());
        assert!(parse_doc(&item(json!({})), false).is_err());
        assert!(
            parse_doc(
                &item(json!({"url": "https://a.example/x.png", "image_id": "5"})),
                false
            )
            .is_err()
        );
        assert!(
            parse_doc(
                &item(json!({"url": "https://a.example/x.png", "proxy_url": "/cdn/external/x/y"})),
                false
            )
            .is_err()
        );
        // The old pattern plus banner_background pair parses unchanged and has no new background.
        let old = parse_doc(
            &json!({"style": {"pattern": "dots", "banner_background": true}}),
            false,
        )
        .unwrap();
        assert!(old.style.background.is_none() && old.style.banner_background);
        // The new shape: kinds, pattern over a banner, and no image id on a pattern.
        assert!(
            parse_doc(
                &json!({"style": {"background": {"kind": "banner", "pattern": "grid"}}}),
                false
            )
            .is_ok()
        );
        assert!(
            parse_doc(
                &json!({"style": {"background": {"kind": "pattern", "image_id": "5"}}}),
                false
            )
            .is_err()
        );
        assert!(parse_doc(&json!({"style": {"background": {"kind": "video"}}}), false).is_err());
        assert!(parse_doc(&json!({"style": {"background": {"kind": "image"}}}), false).is_err());
    }
}
