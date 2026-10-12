use crate::model::ThemePackage;
use crate::registry::TOKENS;
use crate::validate::{ValidationReport, validate};
use std::fmt;

/// Compilation refused because validation found errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub report: ValidationReport,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.report.errors().count();
        write!(f, "theme package has {n} validation error(s)")?;
        if let Some(first) = self.report.errors().next() {
            write!(f, "; first: {}: {}", first.path, first.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for CompileError {}

/// Compile a valid package into one CSS rule:
///
/// ```css
/// [data-theme="my-theme"] { color-scheme: dark; --ui-accent: #fff; }
/// ```
///
/// Output is deterministic (registry order). Validation guarantees ids and values contain
/// no characters that could break out of the declaration. Asset paths are not emitted
/// here: no token references one yet.
pub fn compile_css(pkg: &ThemePackage) -> Result<String, CompileError> {
    let report = validate(pkg);
    if !report.is_valid() {
        return Err(CompileError { report });
    }
    let mut css = format!(
        "[data-theme=\"{}\"] {{\n  color-scheme: {};\n",
        pkg.manifest.id,
        pkg.manifest.scheme.css()
    );
    for spec in TOKENS {
        if let Some(v) = pkg.tokens.get(spec.name) {
            css.push_str("  ");
            css.push_str(spec.name);
            css.push_str(": ");
            css.push_str(v.trim());
            css.push_str(";\n");
        }
    }
    css.push_str("}\n");
    Ok(css)
}
