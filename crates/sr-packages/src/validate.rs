//! Package validation (EXT-PKG-002/003, docs/11 section 4): one function judges a package folder whatever
//! its kind, for the upload route and for `sr-cli package validate`. A package is data only (D-063): the
//! kind is told by its manifest file, every file must be one the kind uses, and every check that depends
//! on the running server (the feature registry, the server-template snapshot) comes in through [`Checks`].
//!
//! | Kind | Manifest | Also |
//! |---|---|---|
//! | `theme` | `theme.json` | `tokens.json`, `assets/`, `fonts/` (sr-theme) |
//! | `profile_layout` | `layout.json` | none |
//! | `emoji_pack`, `sticker_pack`, `sound_pack` | `pack.json` | `emoji/`, `stickers/`, `sounds/` |
//! | `recipe` | `recipe.json` | none |
//! | `server_template` | `template.json` | none |

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use utoipa::ToSchema;

use crate::profile_doc;
use crate::zipfiles::Files;

pub const MAX_RECIPE_FEATURES: usize = 20;
pub const MAX_TEMPLATE_BYTES: usize = 256 * 1024;
const MAX_JSON_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Theme,
    ProfileLayout,
    EmojiPack,
    StickerPack,
    SoundPack,
    Recipe,
    ServerTemplate,
}

impl PackageKind {
    pub const ALL: [PackageKind; 7] = [
        Self::Theme,
        Self::ProfileLayout,
        Self::EmojiPack,
        Self::StickerPack,
        Self::SoundPack,
        Self::Recipe,
        Self::ServerTemplate,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::ProfileLayout => "profile_layout",
            Self::EmojiPack => "emoji_pack",
            Self::StickerPack => "sticker_pack",
            Self::SoundPack => "sound_pack",
            Self::Recipe => "recipe",
            Self::ServerTemplate => "server_template",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// Directory of a media pack's files.
    fn media_dir(self) -> Option<&'static str> {
        match self {
            Self::EmojiPack => Some("emoji/"),
            Self::StickerPack => Some("stickers/"),
            Self::SoundPack => Some("sounds/"),
            _ => None,
        }
    }
}

/// Checks that need the running server. Each returns the reason it refuses.
pub struct Checks<'a> {
    /// A recipe entry: the feature id with its settings. Must know the feature, accept the settings against
    /// its schema and refuse settings that name a channel or role (a recipe is shared across servers).
    pub recipe_feature: &'a dyn Fn(&str, &Map<String, Value>) -> Result<(), String>,
    /// The `serialized_source_guild` of a server template.
    pub template: &'a dyn Fn(&Value) -> Result<(), String>,
}

impl Checks<'static> {
    /// Accept everything the server would check (the local validator, which has no registry).
    pub fn lenient() -> Self {
        Self {
            recipe_feature: &|_, _| Ok(()),
            template: &|_| Ok(()),
        }
    }
}

/// One file of a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = PackageFileInfo)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
}

/// A package that passed every check.
#[derive(Debug, Clone, PartialEq)]
pub struct Validated {
    pub kind: PackageKind,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub files: Vec<FileInfo>,
    /// What the kind needs at runtime, stored with the version (see the table in the module docs).
    pub payload: Value,
    pub warnings: Vec<String>,
    pub total_bytes: u64,
}

fn slug_ok(s: &str) -> bool {
    (3..=48).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

fn version_ok(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()))
}

fn common(
    errs: &mut Vec<String>,
    id: &str,
    name: &str,
    version: &str,
    author: &str,
    description: &str,
) {
    if !slug_ok(id) {
        errs.push("id must be a 3 to 48 character slug (a-z, 0-9, -)".to_owned());
    }
    if name.trim().is_empty() || name.chars().count() > 64 {
        errs.push("name must be 1 to 64 characters".to_owned());
    }
    if !version_ok(version) {
        errs.push("version must look like 1.0.0".to_owned());
    }
    if author.trim().is_empty() || author.chars().count() > 64 {
        errs.push("author must be 1 to 64 characters".to_owned());
    }
    if description.chars().count() > 300 {
        errs.push("description must be at most 300 characters".to_owned());
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutFile {
    id: String,
    name: String,
    version: String,
    author: String,
    #[serde(default)]
    description: String,
    doc: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackItem {
    name: String,
    file: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    kind: String,
    id: String,
    name: String,
    version: String,
    author: String,
    #[serde(default)]
    description: String,
    items: Vec<PackItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeEntry {
    id: String,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    settings: Option<Map<String, Value>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeFile {
    id: String,
    name: String,
    version: String,
    author: String,
    #[serde(default)]
    description: String,
    features: Vec<RecipeEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateFile {
    id: String,
    name: String,
    version: String,
    author: String,
    #[serde(default)]
    description: String,
    serialized_source_guild: Value,
}

fn read<T: for<'de> Deserialize<'de>>(files: &Files, name: &str) -> Result<T, Vec<String>> {
    let bytes = files
        .get(name)
        .ok_or_else(|| vec![format!("{name} is missing")])?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(vec![format!("{name} is larger than 512 KiB")]);
    }
    serde_json::from_slice(bytes).map_err(|e| vec![format!("{name}: {e}")])
}

/// Which kind a folder is, by its manifest file.
pub fn detect_kind(files: &Files) -> Result<PackageKind, String> {
    let mut found = Vec::new();
    if files.contains_key("theme.json") {
        found.push(PackageKind::Theme);
    }
    if files.contains_key("layout.json") {
        found.push(PackageKind::ProfileLayout);
    }
    if files.contains_key("recipe.json") {
        found.push(PackageKind::Recipe);
    }
    if files.contains_key("template.json") {
        found.push(PackageKind::ServerTemplate);
    }
    if let Some(p) = files.get("pack.json") {
        let kind = serde_json::from_slice::<Value>(p)
            .ok()
            .and_then(|v| v.get("kind")?.as_str().map(str::to_owned))
            .and_then(|k| PackageKind::parse(&k))
            .filter(|k| k.media_dir().is_some());
        match kind {
            Some(k) => found.push(k),
            None => {
                return Err(
                    "pack.json needs a kind of emoji_pack, sticker_pack or sound_pack".to_owned(),
                );
            }
        }
    }
    match found.as_slice() {
        [k] => Ok(*k),
        [] => Err("no manifest: expected theme.json, layout.json, pack.json, recipe.json or template.json".to_owned()),
        _ => Err("more than one manifest file".to_owned()),
    }
}

/// Validate a package folder of any kind.
pub fn validate(files: &Files, checks: &Checks) -> Result<Validated, Vec<String>> {
    let kind = detect_kind(files).map_err(|e| vec![e])?;
    let mut errs = Vec::new();
    let mut warnings = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let manifest = match kind {
        PackageKind::Theme => theme(files, &mut errs, &mut warnings, &mut used)?,
        PackageKind::ProfileLayout => layout(files, &mut errs, &mut used)?,
        PackageKind::EmojiPack | PackageKind::StickerPack | PackageKind::SoundPack => {
            pack(kind, files, &mut errs, &mut used)?
        }
        PackageKind::Recipe => recipe(files, checks, &mut errs, &mut used)?,
        PackageKind::ServerTemplate => template(files, checks, &mut errs, &mut used)?,
    };
    for path in files.keys() {
        if !used.contains(path) && !path.ends_with(".txt") {
            errs.push(format!("{path}: is not used by the package"));
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    let total_bytes = files.values().map(|v| v.len() as u64).sum();
    Ok(Validated {
        kind,
        slug: manifest.0,
        name: manifest.1,
        version: manifest.2,
        author: manifest.3,
        description: manifest.4,
        files: files
            .iter()
            .map(|(p, d)| FileInfo {
                path: p.clone(),
                size: d.len() as u64,
            })
            .collect(),
        payload: manifest.5,
        warnings,
        total_bytes,
    })
}

type Manifest = (String, String, String, String, String, Value);

fn theme(
    files: &Files,
    errs: &mut Vec<String>,
    warnings: &mut Vec<String>,
    used: &mut BTreeSet<String>,
) -> Result<Manifest, Vec<String>> {
    let pkg = sr_theme::package_from_files(files)?;
    let report = sr_theme::validate(&pkg);
    let file_report = sr_theme::validate_files(&pkg, files);
    for i in report.issues.iter().chain(file_report.issues.iter()) {
        let line = format!("{}: {}", i.path, i.message);
        match i.severity {
            sr_theme::Severity::Error => errs.push(line),
            sr_theme::Severity::Warning => warnings.push(line),
        }
    }
    used.insert("theme.json".to_owned());
    used.insert("tokens.json".to_owned());
    for a in &pkg.assets {
        used.insert(a.path.clone());
    }
    for f in &pkg.fonts {
        used.insert(f.path.clone());
        used.insert(f.license.clone());
    }
    let css = sr_theme::compile_css(&pkg).unwrap_or_default();
    let m = &pkg.manifest;
    Ok((
        m.id.clone(),
        m.name.clone(),
        m.version.clone(),
        m.author.clone(),
        String::new(),
        json!({
            "scheme": match m.scheme { sr_theme::ThemeScheme::Light => "light", sr_theme::ThemeScheme::Dark => "dark" },
            "targets": m.targets,
            "extends": m.extends,
            "tokens": pkg.tokens,
            "css": css,
        }),
    ))
}

fn layout(
    files: &Files,
    errs: &mut Vec<String>,
    used: &mut BTreeSet<String>,
) -> Result<Manifest, Vec<String>> {
    let f: LayoutFile = read(files, "layout.json")?;
    used.insert("layout.json".to_owned());
    common(errs, &f.id, &f.name, &f.version, &f.author, &f.description);
    let doc = match profile_doc::parse_doc(&f.doc, true) {
        Ok(d) => d,
        Err(ps) => {
            errs.extend(
                ps.into_iter()
                    .map(|p| format!("doc.{}: {}", p.path, p.message)),
            );
            profile_doc::ProfileDoc::default()
        }
    };
    if doc.blocks.is_empty() {
        errs.push("doc.blocks: a layout needs at least one block".to_owned());
    }
    Ok((
        f.id,
        f.name,
        f.version,
        f.author,
        f.description,
        json!({ "doc": doc }),
    ))
}

/// Pixel size of a PNG or GIF, from the header.
fn image_dims(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        let w = u32::from_be_bytes(data[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(data[20..24].try_into().ok()?);
        return Some((w, h));
    }
    if data.len() >= 10 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        let w = u32::from(u16::from_le_bytes(data[6..8].try_into().ok()?));
        let h = u32::from(u16::from_le_bytes(data[8..10].try_into().ok()?));
        return Some((w, h));
    }
    None
}

fn is_webp(d: &[u8]) -> bool {
    d.len() > 12 && &d[0..4] == b"RIFF" && &d[8..12] == b"WEBP"
}

fn is_mp3(d: &[u8]) -> bool {
    d.starts_with(b"ID3") || (d.len() > 2 && d[0] == 0xFF && d[1] & 0xE0 == 0xE0)
}

fn pack(
    kind: PackageKind,
    files: &Files,
    errs: &mut Vec<String>,
    used: &mut BTreeSet<String>,
) -> Result<Manifest, Vec<String>> {
    let f: PackFile = read(files, "pack.json")?;
    used.insert("pack.json".to_owned());
    common(errs, &f.id, &f.name, &f.version, &f.author, &f.description);
    if f.kind != kind.as_str() {
        errs.push("pack.json: kind does not match".to_owned());
    }
    let dir = kind.media_dir().unwrap_or("");
    let (max_items, max_bytes, max_px) = match kind {
        PackageKind::EmojiPack => (100, 256 * 1024, 512),
        PackageKind::StickerPack => (40, 512 * 1024, 1024),
        _ => (40, 512 * 1024, 0),
    };
    if f.items.is_empty() || f.items.len() > max_items {
        errs.push(format!("items: between 1 and {max_items} items"));
    }
    let mut names = BTreeSet::new();
    for it in &f.items {
        let ok_name = (2..=32).contains(&it.name.len())
            && it
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !ok_name {
            errs.push(format!(
                "{}: names use a-z, 0-9 and _, 2 to 32 characters",
                it.name
            ));
        }
        if !names.insert(it.name.clone()) {
            errs.push(format!("{}: is listed twice", it.name));
        }
        if !it.file.starts_with(dir) {
            errs.push(format!("{}: files of this pack sit under {dir}", it.file));
            continue;
        }
        let Some(data) = files.get(&it.file) else {
            errs.push(format!("{}: is missing", it.file));
            continue;
        };
        used.insert(it.file.clone());
        if data.len() > max_bytes {
            errs.push(format!("{}: larger than {} KiB", it.file, max_bytes / 1024));
        }
        if kind == PackageKind::SoundPack {
            if !(data.starts_with(b"OggS") || is_mp3(data)) {
                errs.push(format!("{}: must be an Ogg or MP3 file", it.file));
            }
        } else if let Some((w, h)) = image_dims(data) {
            if w == 0 || h == 0 || w > max_px || h > max_px {
                errs.push(format!(
                    "{}: larger than {max_px} by {max_px} pixels",
                    it.file
                ));
            }
        } else if !is_webp(data) {
            errs.push(format!("{}: must be a PNG, GIF or WebP image", it.file));
        }
    }
    Ok((
        f.id,
        f.name,
        f.version,
        f.author,
        f.description,
        json!({ "items": f.items.iter().map(|i| json!({"name": i.name, "file": i.file})).collect::<Vec<_>>() }),
    ))
}

fn recipe(
    files: &Files,
    checks: &Checks,
    errs: &mut Vec<String>,
    used: &mut BTreeSet<String>,
) -> Result<Manifest, Vec<String>> {
    let f: RecipeFile = read(files, "recipe.json")?;
    used.insert("recipe.json".to_owned());
    common(errs, &f.id, &f.name, &f.version, &f.author, &f.description);
    if f.features.is_empty() || f.features.len() > MAX_RECIPE_FEATURES {
        errs.push(format!(
            "features: between 1 and {MAX_RECIPE_FEATURES} entries"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for e in &f.features {
        if !seen.insert(e.id.clone()) {
            errs.push(format!("{}: is listed twice", e.id));
        }
        if e.enabled.is_none() && e.settings.is_none() {
            errs.push(format!("{}: sets neither enabled nor settings", e.id));
        }
        let empty = Map::new();
        if let Err(why) = (checks.recipe_feature)(&e.id, e.settings.as_ref().unwrap_or(&empty)) {
            errs.push(format!("{}: {why}", e.id));
        }
        out.push(json!({"id": e.id, "enabled": e.enabled, "settings": e.settings}));
    }
    Ok((
        f.id,
        f.name,
        f.version,
        f.author,
        f.description,
        json!({ "features": out }),
    ))
}

fn template(
    files: &Files,
    checks: &Checks,
    errs: &mut Vec<String>,
    used: &mut BTreeSet<String>,
) -> Result<Manifest, Vec<String>> {
    let f: TemplateFile = read(files, "template.json")?;
    used.insert("template.json".to_owned());
    common(errs, &f.id, &f.name, &f.version, &f.author, &f.description);
    if serde_json::to_vec(&f.serialized_source_guild).map_or(true, |b| b.len() > MAX_TEMPLATE_BYTES)
    {
        errs.push("serialized_source_guild: larger than 256 KiB".to_owned());
    } else if let Err(why) = (checks.template)(&f.serialized_source_guild) {
        errs.push(format!("serialized_source_guild: {why}"));
    }
    Ok((
        f.id,
        f.name,
        f.version,
        f.author,
        f.description,
        json!({ "serialized_source_guild": f.serialized_source_guild }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &[u8])]) -> Files {
        list.iter()
            .map(|(p, d)| ((*p).to_owned(), d.to_vec()))
            .collect()
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v
    }

    const HEAD: &str = r#""id": "my-pack", "name": "My pack", "version": "1.0.0", "author": "me""#;

    #[test]
    fn kinds_are_told_by_the_manifest() {
        assert_eq!(
            detect_kind(&files(&[("recipe.json", b"{}")])).unwrap(),
            PackageKind::Recipe
        );
        assert!(detect_kind(&files(&[("x.json", b"{}")])).is_err());
        assert!(detect_kind(&files(&[("recipe.json", b"{}"), ("layout.json", b"{}")])).is_err());
        let p = files(&[("pack.json", br#"{"kind": "sound_pack"}"#)]);
        assert_eq!(detect_kind(&p).unwrap(), PackageKind::SoundPack);
        assert!(detect_kind(&files(&[("pack.json", br#"{"kind": "theme"}"#)])).is_err());
    }

    #[test]
    fn an_emoji_pack_is_checked_file_by_file() {
        let manifest = format!(
            r#"{{"kind": "emoji_pack", {HEAD}, "items": [{{"name": "wave", "file": "emoji/wave.png"}}]}}"#
        );
        let good = files(&[
            ("pack.json", manifest.as_bytes()),
            ("emoji/wave.png", &png(64, 64)),
        ]);
        let v = validate(&good, &Checks::lenient()).unwrap();
        assert_eq!(v.kind, PackageKind::EmojiPack);
        assert_eq!(v.slug, "my-pack");
        assert_eq!(v.files.len(), 2);

        let big = files(&[
            ("pack.json", manifest.as_bytes()),
            ("emoji/wave.png", &png(900, 900)),
        ]);
        assert!(validate(&big, &Checks::lenient()).unwrap_err()[0].contains("pixels"));
        let fake = files(&[
            ("pack.json", manifest.as_bytes()),
            ("emoji/wave.png", b"<svg onload=x>"),
        ]);
        assert!(validate(&fake, &Checks::lenient()).is_err());
        let extra = files(&[
            ("pack.json", manifest.as_bytes()),
            ("emoji/wave.png", &png(64, 64)),
            ("emoji/other.png", &png(64, 64)),
        ]);
        assert!(validate(&extra, &Checks::lenient()).unwrap_err()[0].contains("not used"));
        let missing = files(&[("pack.json", manifest.as_bytes())]);
        assert!(validate(&missing, &Checks::lenient()).unwrap_err()[0].contains("missing"));
    }

    #[test]
    fn a_recipe_asks_the_server_about_every_feature() {
        let recipe = format!(
            r#"{{{HEAD}, "features": [{{"id": "lounge.starboard", "enabled": true, "settings": {{"threshold": 3}}}}, {{"id": "no.such"}}]}}"#
        );
        let f = files(&[("recipe.json", recipe.as_bytes())]);
        let checks = Checks {
            recipe_feature: &|id, _| {
                if id == "no.such" {
                    Err("unknown feature".to_owned())
                } else {
                    Ok(())
                }
            },
            template: &|_| Ok(()),
        };
        let errs = validate(&f, &checks).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("no.such: unknown feature")));
        assert!(
            errs.iter()
                .any(|e| e.contains("neither enabled nor settings"))
        );
        let ok =
            format!(r#"{{{HEAD}, "features": [{{"id": "lounge.starboard", "enabled": true}}]}}"#);
        assert!(validate(&files(&[("recipe.json", ok.as_bytes())]), &checks).is_ok());
    }

    #[test]
    fn a_layout_holds_a_template_document() {
        let doc = r#"{"blocks": [{"id": "a", "type": "about", "x": 0, "y": 0, "w": 12, "h": 3}]}"#;
        let ok = format!(r#"{{{HEAD}, "doc": {doc}}}"#);
        let v = validate(
            &files(&[("layout.json", ok.as_bytes())]),
            &Checks::lenient(),
        )
        .unwrap();
        assert_eq!(v.kind, PackageKind::ProfileLayout);
        let empty = format!(r#"{{{HEAD}, "doc": {{}}}}"#);
        assert!(
            validate(
                &files(&[("layout.json", empty.as_bytes())]),
                &Checks::lenient()
            )
            .is_err()
        );
        let with_text = r#"{"blocks": [{"id": "a", "type": "about", "x": 0, "y": 0, "w": 12, "h": 3, "config": {"text": "hi"}}]}"#;
        let bad = format!(r#"{{{HEAD}, "doc": {with_text}}}"#);
        assert!(
            validate(
                &files(&[("layout.json", bad.as_bytes())]),
                &Checks::lenient()
            )
            .is_err()
        );
    }

    #[test]
    fn a_theme_goes_through_sr_theme() {
        let manifest = br##"{"id": "ocean-night", "name": "Ocean Night", "version": "1.0.0", "author": "me", "targets": ["lounge"], "scheme": "dark"}"##;
        let tokens = br##"{"--ui-accent": "#5865f2"}"##;
        let f = files(&[("theme.json", manifest), ("tokens.json", tokens)]);
        let v = validate(&f, &Checks::lenient()).unwrap();
        assert_eq!(v.kind, PackageKind::Theme);
        assert!(
            v.payload["css"]
                .as_str()
                .unwrap()
                .contains("data-theme=\"ocean-night\"")
        );
        let bad = br##"{"--ui-accent": "url(https://x)"}"##;
        let f = files(&[("theme.json", manifest), ("tokens.json", bad)]);
        assert!(validate(&f, &Checks::lenient()).is_err());
    }

    #[test]
    fn a_template_is_checked_by_the_server() {
        let t = format!(r#"{{{HEAD}, "serialized_source_guild": {{"roles": []}}}}"#);
        let f = files(&[("template.json", t.as_bytes())]);
        let checks = Checks {
            recipe_feature: &|_, _| Ok(()),
            template: &|_| Err("no settings".to_owned()),
        };
        assert!(validate(&f, &checks).unwrap_err()[0].contains("no settings"));
        assert!(validate(&f, &Checks::lenient()).is_ok());
    }
}
