use crate::model::ThemePackage;
use crate::registry::{TokenKind, token_spec};
use serde::Serialize;

/// Reserved ids of the built-in themes.
pub const BUILTIN_THEME_IDS: &[&str] = &[
    "lounge-dark",
    "lounge-light",
    "lounge-ash",
    "lounge-onyx",
    "sideroom",
    "sideroom-broadcast",
];

/// Valid `targets` values (the platform sections).
const SECTIONS: &[&str] = &[
    "lounge",
    "staff",
    "developer",
    "entrance",
    "cinema",
    "library",
    "support",
    "live-tv",
];

/// 2 MB per image, 10 MB per package (docs/10 section 4).
pub const MAX_ASSET_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_TOTAL_ASSET_BYTES: u64 = 10 * 1024 * 1024;
/// Per-font cap. Not in the docs; see docs/open-questions.md.
pub const MAX_FONT_BYTES: u64 = 1024 * 1024;

const MAX_VALUE_LEN: usize = 256;
const WCAG_AA_TEXT: f64 = 4.5;

/// Token pairs checked for WCAG AA text contrast (foreground, background).
const CONTRAST_PAIRS: &[(&str, &str)] = &[
    ("--ui-text", "--ui-bg"),
    ("--ui-text-muted", "--ui-bg"),
    ("--ui-accent-ink", "--ui-accent"),
    ("--ui-danger-ink", "--ui-danger"),
    ("--text-normal", "--bg-base-low"),
    ("--sr-text", "--sr-bg-0"),
    ("--sr-gold-ink", "--sr-gold"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    /// Stable machine code, e.g. `unknown-token`.
    pub code: &'static str,
    /// Where: `manifest.id`, `tokens.--ui-accent`, `assets[0].path`.
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors().next().is_none()
    }
    pub fn errors(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error)
    }
    pub fn warnings(&self) -> impl Iterator<Item = &Issue> {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
    }
    fn push(&mut self, severity: Severity, code: &'static str, path: String, message: String) {
        self.issues.push(Issue {
            severity,
            code,
            path,
            message,
        });
    }
    fn err(&mut self, code: &'static str, path: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Error, code, path.into(), message.into());
    }
    fn warn(&mut self, code: &'static str, path: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Warning, code, path.into(), message.into());
    }
}

/// Validate a package: manifest, known tokens and value grammar, assets, fonts,
/// and contrast (warnings only).
pub fn validate(pkg: &ThemePackage) -> ValidationReport {
    let mut r = ValidationReport::default();
    manifest(pkg, &mut r);
    tokens(pkg, &mut r);
    assets(pkg, &mut r);
    fonts(pkg, &mut r);
    contrast(pkg, &mut r);
    r
}

fn manifest(pkg: &ThemePackage, r: &mut ValidationReport) {
    let m = &pkg.manifest;
    let id_ok = (3..=48).contains(&m.id.len())
        && m.id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !m.id.starts_with('-')
        && !m.id.ends_with('-');
    if !id_ok {
        r.err(
            "invalid-id",
            "manifest.id",
            "id must be a 3-48 character lowercase slug (a-z, 0-9, -)",
        );
    } else if BUILTIN_THEME_IDS.contains(&m.id.as_str()) {
        r.err(
            "reserved-id",
            "manifest.id",
            "id collides with a built-in theme",
        );
    }
    if m.name.trim().is_empty() || m.name.chars().count() > 64 {
        r.err(
            "invalid-name",
            "manifest.name",
            "name must be 1-64 characters",
        );
    }
    if m.author.trim().is_empty() || m.author.chars().count() > 64 {
        r.err(
            "invalid-author",
            "manifest.author",
            "author must be 1-64 characters",
        );
    }
    if !is_semver(&m.version) {
        r.err(
            "invalid-version",
            "manifest.version",
            "version must be MAJOR.MINOR.PATCH",
        );
    }
    if m.targets.is_empty() {
        r.err(
            "no-targets",
            "manifest.targets",
            "at least one target is required",
        );
    }
    for (i, t) in m.targets.iter().enumerate() {
        if !SECTIONS.contains(&t.as_str()) {
            r.err(
                "unknown-target",
                format!("manifest.targets[{i}]"),
                format!("unknown target `{t}`"),
            );
        }
    }
    if let Some(e) = &m.extends
        && !BUILTIN_THEME_IDS.contains(&e.as_str())
    {
        r.err(
            "unknown-extends",
            "manifest.extends",
            format!("`{e}` is not a built-in theme"),
        );
    }
}

fn is_semver(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 9
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))
        })
}

fn tokens(pkg: &ThemePackage, r: &mut ValidationReport) {
    for (name, value) in &pkg.tokens {
        let path = format!("tokens.{name}");
        let Some(spec) = token_spec(name) else {
            r.err("unknown-token", path, format!("unknown token `{name}`"));
            continue;
        };
        if let Err(msg) = check_value(spec.kind, value) {
            r.err("invalid-value", path, msg);
        }
    }
}

/// Characters and substrings that can never appear in a token value: they would allow
/// escaping the declaration, injecting a rule, or loading a resource.
fn check_common(v: &str) -> Result<(), String> {
    if v.trim().is_empty() {
        return Err("value is empty".into());
    }
    if v.len() > MAX_VALUE_LEN {
        return Err(format!("value longer than {MAX_VALUE_LEN} characters"));
    }
    if v.chars().any(|c| c.is_control() || ";{}<>\\@!".contains(c)) {
        return Err("value contains a forbidden character".into());
    }
    let lower = v.to_ascii_lowercase();
    for bad in [
        "url(",
        "image(",
        "image-set(",
        "expression(",
        "var(",
        "/*",
        "//",
        "javascript:",
    ] {
        if lower.contains(bad) {
            return Err(format!("`{bad}` is not allowed (no URLs, no references)"));
        }
    }
    Ok(())
}

fn check_value(kind: TokenKind, v: &str) -> Result<(), String> {
    check_common(v)?;
    let v = v.trim();
    match kind {
        TokenKind::Color => check_color(v),
        TokenKind::Length => check_length(v),
        TokenKind::Number => v
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|_| ())
            .ok_or_else(|| "expected a number".to_string()),
        TokenKind::Font => {
            if v.chars()
                .all(|c| c.is_ascii_alphanumeric() || " ,\"'-_.".contains(c))
            {
                Ok(())
            } else {
                Err("font list may contain only letters, digits, spaces, quotes, `,`, `-`, `_`, `.`".into())
            }
        }
        TokenKind::Shadow => {
            if v.chars()
                .all(|c| c.is_ascii_alphanumeric() || " .,#%()/-".contains(c))
            {
                Ok(())
            } else {
                Err("shadow contains unsupported characters".into())
            }
        }
        TokenKind::Gradient => {
            let ok_start =
                v.starts_with("linear-gradient(") || v.starts_with("repeating-linear-gradient(");
            let ok_chars = v
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " .,#%()/-".contains(c));
            if ok_start && ok_chars && v.ends_with(')') {
                Ok(())
            } else {
                Err("expected a (repeating-)linear-gradient() of plain colors".into())
            }
        }
    }
}

fn check_color(v: &str) -> Result<(), String> {
    if let Some(hex) = v.strip_prefix('#') {
        return if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(())
        } else {
            Err("invalid hex color".into())
        };
    }
    let lower = v.to_ascii_lowercase();
    if lower == "transparent" {
        return Ok(());
    }
    for f in ["rgba(", "rgb(", "hsla(", "hsl("] {
        if let Some(rest) = lower.strip_prefix(f) {
            let ok = rest.strip_suffix(')').is_some_and(|inner| {
                inner.chars().any(|c| c.is_ascii_digit())
                    && inner.chars().all(|c| {
                        c.is_ascii_digit() || " .,%/-+".contains(c) || c == 'd' || c == 'e'
                    })
            });
            return if ok {
                Ok(())
            } else {
                Err(format!("invalid {f}) color"))
            };
        }
    }
    Err("expected a hex color, rgb()/hsl() or `transparent`".into())
}

fn check_length(v: &str) -> Result<(), String> {
    if v == "0" {
        return Ok(());
    }
    for unit in ["px", "rem", "em", "%"] {
        if let Some(n) = v.strip_suffix(unit)
            && n.parse::<f64>().is_ok_and(f64::is_finite)
        {
            return Ok(());
        }
    }
    Err("expected `0` or a number with px, rem, em or %".into())
}

fn path_ok(p: &str, prefix: &str) -> Result<(), &'static str> {
    if p.contains("://") || p.starts_with("data:") || p.starts_with("//") {
        return Err("URLs are not allowed; use a package-relative path");
    }
    if p.is_empty() || p.chars().any(|c| c.is_control()) {
        return Err("invalid path");
    }
    if p.starts_with('/') || p.contains('\\') || p.split('/').any(|s| s == ".." || s == ".") {
        return Err("path must be package-relative with no `..`");
    }
    if !p.starts_with(prefix) {
        return Err("path is outside the expected directory");
    }
    Ok(())
}

fn assets(pkg: &ThemePackage, r: &mut ValidationReport) {
    let mut total: u64 = 0;
    for (i, a) in pkg.assets.iter().enumerate() {
        let base = format!("assets[{i}]");
        if let Err(m) = path_ok(&a.path, "assets/") {
            r.err("invalid-asset-path", format!("{base}.path"), m);
        }
        if !matches!(a.mime.as_str(), "image/png" | "image/webp" | "image/avif") {
            r.err(
                "invalid-asset-type",
                format!("{base}.mime"),
                "assets must be PNG, WebP or AVIF",
            );
        }
        if a.bytes > MAX_ASSET_BYTES {
            r.err(
                "asset-too-large",
                format!("{base}.bytes"),
                "asset exceeds 2 MB",
            );
        }
        total = total.saturating_add(a.bytes);
    }
    for f in &pkg.fonts {
        total = total.saturating_add(f.bytes);
    }
    if total > MAX_TOTAL_ASSET_BYTES {
        r.err(
            "package-too-large",
            "assets",
            "assets and fonts exceed 10 MB in total",
        );
    }
}

fn fonts(pkg: &ThemePackage, r: &mut ValidationReport) {
    for (i, f) in pkg.fonts.iter().enumerate() {
        let base = format!("fonts[{i}]");
        if let Err(m) = path_ok(&f.path, "fonts/") {
            r.err("invalid-font-path", format!("{base}.path"), m);
        } else if !f.path.to_ascii_lowercase().ends_with(".woff2") {
            r.err(
                "invalid-font-type",
                format!("{base}.path"),
                "fonts must be WOFF2",
            );
        }
        if f.bytes > MAX_FONT_BYTES {
            r.err(
                "font-too-large",
                format!("{base}.bytes"),
                "font exceeds 1 MB",
            );
        }
        if f.family.trim().is_empty()
            || !f
                .family
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " -_.".contains(c))
        {
            r.err(
                "invalid-font-family",
                format!("{base}.family"),
                "family may contain only letters, digits, spaces, `-`, `_`, `.`",
            );
        }
        if f.license.trim().is_empty() {
            r.err(
                "missing-font-license",
                format!("{base}.license"),
                "a licence file is required for every font",
            );
        } else if let Err(m) = path_ok(&f.license, "fonts/") {
            r.err("invalid-font-license", format!("{base}.license"), m);
        }
    }
}

fn contrast(pkg: &ThemePackage, r: &mut ValidationReport) {
    for (fg, bg) in CONTRAST_PAIRS {
        let (Some(a), Some(b)) = (
            pkg.tokens.get(*fg).and_then(|v| parse_hex(v)),
            pkg.tokens.get(*bg).and_then(|v| parse_hex(v)),
        ) else {
            continue;
        };
        let ratio = contrast_ratio(a, b);
        if ratio < WCAG_AA_TEXT {
            r.warn(
                "low-contrast",
                format!("tokens.{fg}"),
                format!("contrast with {bg} is {ratio:.2}:1, below WCAG AA 4.5:1"),
            );
        }
    }
}

/// Opaque `#rgb` / `#rrggbb` only (alpha colors are skipped).
fn parse_hex(v: &str) -> Option<[u8; 3]> {
    let h = v.trim().strip_prefix('#')?;
    let full: String = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        6 => h.to_string(),
        _ => return None,
    };
    let n = u32::from_str_radix(&full, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn luminance(c: [u8; 3]) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile_css, parse_package};

    fn pkg(tokens: &str) -> ThemePackage {
        parse_package(&format!(
            r#"{{"manifest":{{"id":"my-theme","name":"Mine","version":"1.0.0","author":"a",
            "targets":["lounge"],"scheme":"dark"}},"tokens":{tokens}}}"#
        ))
        .expect("fixture parses")
    }

    fn codes(r: &ValidationReport) -> Vec<&'static str> {
        r.errors().map(|i| i.code).collect()
    }

    #[test]
    fn valid_package_compiles_in_registry_order() {
        let p = pkg(r##"{"--ui-text":"#fff","--ui-bg":"#000","--ui-radius":"4px"}"##);
        assert!(validate(&p).is_valid());
        let css = compile_css(&p).expect("compiles");
        assert_eq!(
            css,
            "[data-theme=\"my-theme\"] {\n  color-scheme: dark;\n  --ui-bg: #000;\n  --ui-text: #fff;\n  --ui-radius: 4px;\n}\n"
        );
    }

    #[test]
    fn rejects_unknown_tokens_and_injection() {
        let p = pkg(
            r##"{"--nope":"#fff","--ui-bg":"red; } body { display:none","--ui-text":"url(x)","--ui-accent":"#12"}"##,
        );
        let r = validate(&p);
        assert_eq!(
            codes(&r),
            vec![
                "unknown-token",
                "invalid-value",
                "invalid-value",
                "invalid-value"
            ]
        );
        assert!(compile_css(&p).is_err());
    }

    #[test]
    fn low_contrast_is_a_warning_only() {
        let p = pkg(r##"{"--ui-text":"#222","--ui-bg":"#111"}"##);
        let r = validate(&p);
        assert!(r.is_valid());
        assert_eq!(r.warnings().count(), 1);
    }

    #[test]
    fn manifest_rules() {
        let mut p = pkg("{}");
        p.manifest.id = "sideroom".into();
        p.manifest.version = "1.0".into();
        p.manifest.targets = vec!["board".into()];
        p.manifest.extends = Some("nope".into());
        assert_eq!(
            codes(&validate(&p)),
            vec![
                "reserved-id",
                "invalid-version",
                "unknown-target",
                "unknown-extends"
            ]
        );
    }

    #[test]
    fn asset_and_font_rules() {
        let mut p = pkg("{}");
        p.assets = vec![
            crate::AssetRef {
                path: "https://x/y.png".into(),
                mime: "image/gif".into(),
                bytes: MAX_ASSET_BYTES + 1,
            },
            crate::AssetRef {
                path: "assets/../../etc".into(),
                mime: "image/png".into(),
                bytes: 1,
            },
        ];
        p.fonts = vec![crate::FontRef {
            path: "fonts/a.ttf".into(),
            family: "A".into(),
            bytes: 1,
            license: String::new(),
        }];
        assert_eq!(
            codes(&validate(&p)),
            vec![
                "invalid-asset-path",
                "invalid-asset-type",
                "asset-too-large",
                "invalid-asset-path",
                "invalid-font-type",
                "missing-font-license"
            ]
        );
    }

    #[test]
    fn total_size_cap() {
        let mut p = pkg("{}");
        p.assets = (0..6)
            .map(|i| crate::AssetRef {
                path: format!("assets/{i}.png"),
                mime: "image/png".into(),
                bytes: MAX_ASSET_BYTES,
            })
            .collect();
        assert_eq!(codes(&validate(&p)), vec!["package-too-large"]);
    }

    #[test]
    fn unknown_fields_rejected() {
        assert!(parse_package(r#"{"manifest":{},"x":1}"#).is_err());
    }
}
