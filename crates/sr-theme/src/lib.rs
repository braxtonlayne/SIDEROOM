//! sr-theme: theme package schema, validator and CSS-variable compiler
//! (docs/10 section 4, docs/11 section 2). wasm-compatible: pure `std`, no I/O.
//!
//! A theme is a declarative package: a manifest plus token overrides plus asset and
//! font metadata. It never contains code. [`validate`] checks it, [`compile_css`]
//! turns a valid package into a CSS rule keyed on `[data-theme="<id>"]`, and
//! [`json_schema`] emits the JSON Schema that TypeScript types are generated from.

mod compile;
mod emit;
mod model;
mod registry;
mod sniff;
mod validate;

pub use compile::{CompileError, compile_css};
pub use emit::{TS_OUTPUT_PATH, typescript};
pub use model::{AssetRef, FontRef, ThemeManifest, ThemePackage, ThemeScheme};
pub use registry::{TOKENS, TokenKind, TokenSpec, token_spec};
pub use sniff::{MAX_LICENSE_BYTES, validate_files};
pub use validate::{
    BUILTIN_THEME_IDS, Issue, MAX_ASSET_BYTES, MAX_FONT_BYTES, MAX_TOTAL_ASSET_BYTES, Severity,
    ValidationReport, validate,
};

/// Parse a package from JSON. Unknown fields are rejected so typos surface early.
pub fn parse_package(json: &str) -> Result<ThemePackage, serde_json::Error> {
    serde_json::from_str(json)
}

/// JSON Schema (draft 2020-12) for [`ThemePackage`], pretty-printed.
pub fn json_schema() -> String {
    let schema = schemars::schema_for!(ThemePackage);
    // Serializing a schema value cannot fail (string keys only).
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}
