use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Light or dark: drives the CSS `color-scheme` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ThemeScheme {
    Light,
    Dark,
}

impl ThemeScheme {
    pub(crate) fn css(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// `theme.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeManifest {
    /// Lowercase slug (`a-z`, `0-9`, `-`), 3 to 48 characters. Built-in theme ids are reserved.
    pub id: String,
    /// Display name, 1 to 64 characters.
    pub name: String,
    /// Semantic version, `MAJOR.MINOR.PATCH`.
    pub version: String,
    /// Author display name.
    pub author: String,
    /// Sections the theme applies to (`lounge`, `cinema`, ...). At least one.
    pub targets: Vec<String>,
    pub scheme: ThemeScheme,
    /// Built-in theme this one builds on; unspecified tokens fall through to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
}

/// An image asset, referenced by package-relative path. Contents are checked at upload;
/// the validator checks the declared metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssetRef {
    /// Package-relative path, e.g. `assets/pattern.webp`. No URLs, no `..`.
    pub path: String,
    /// MIME type: `image/png`, `image/webp` or `image/avif`.
    pub mime: String,
    pub bytes: u64,
}

/// A bundled font file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontRef {
    /// Package-relative path to a `.woff2` file under `fonts/`.
    pub path: String,
    pub family: String,
    pub bytes: u64,
    /// Package-relative path to the licence file under `fonts/`. Required.
    pub license: String,
}

/// A whole theme package as validated by the platform (manifest + `tokens.json` + file metadata).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemePackage {
    pub manifest: ThemeManifest,
    /// Token overrides: CSS custom property name (`--ui-accent`) to value.
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
    #[serde(default)]
    pub assets: Vec<AssetRef>,
    #[serde(default)]
    pub fonts: Vec<FontRef>,
}
