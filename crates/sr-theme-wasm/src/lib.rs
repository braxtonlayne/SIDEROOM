//! Browser bindings for `sr-theme`: validate and compile theme packages exactly as the
//! server does before upload. Structured results are returned as JSON strings; the
//! typed facade in `packages/wasm/src` parses them.

use wasm_bindgen::prelude::*;

/// Validate a theme package (JSON). Returns `{ "valid": bool, "issues": [...] }`.
/// A package that does not parse yields a single `invalid-json` error issue.
pub fn validate_json(package_json: &str) -> String {
    let value = match sr_theme::parse_package(package_json) {
        Ok(pkg) => {
            let report = sr_theme::validate(&pkg);
            serde_json::json!({ "valid": report.is_valid(), "issues": report.issues })
        }
        Err(e) => serde_json::json!({
            "valid": false,
            "issues": [{
                "severity": "error",
                "code": "invalid-json",
                "path": "",
                "message": e.to_string(),
            }],
        }),
    };
    value.to_string()
}

/// Compile a theme package (JSON) to CSS, or return the error message.
pub fn compile_json(package_json: &str) -> Result<String, String> {
    let pkg = sr_theme::parse_package(package_json).map_err(|e| e.to_string())?;
    sr_theme::compile_css(&pkg).map_err(|e| e.to_string())
}

/// Validate a theme package. See [`validate_json`].
#[wasm_bindgen(js_name = validateTheme)]
pub fn validate_theme(package_json: &str) -> String {
    validate_json(package_json)
}

/// Compile a theme package to CSS. Throws if the package is invalid.
#[wasm_bindgen(js_name = compileTheme)]
pub fn compile_theme(package_json: &str) -> Result<String, JsError> {
    compile_json(package_json).map_err(|e| JsError::new(&e))
}

/// The JSON Schema for theme packages.
#[wasm_bindgen(js_name = themeSchema)]
pub fn theme_schema() -> String {
    sr_theme::json_schema()
}

/// Names of every settable token.
#[wasm_bindgen(js_name = tokenNames)]
pub fn token_names() -> Vec<String> {
    sr_theme::TOKENS
        .iter()
        .map(|t| t.name.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r##"{"manifest":{"id":"my-theme","name":"M","version":"1.0.0","author":"a",
        "targets":["lounge"],"scheme":"light"},"tokens":{"--ui-bg":"#fff"}}"##;

    #[test]
    fn validates_and_compiles() {
        assert!(validate_json(GOOD).contains("\"valid\":true"));
        let css = compile_json(GOOD).expect("compiles");
        assert!(css.contains("--ui-bg: #fff;"));
    }

    #[test]
    fn bad_json_is_reported_not_thrown() {
        let out = validate_json("{");
        assert!(out.contains("invalid-json"));
        assert!(compile_json("{").is_err());
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    /// The `JsError` path is only constructible on the wasm target.
    #[wasm_bindgen_test]
    fn compile_theme_throws_js_error_on_garbage() {
        assert!(compile_theme("not json").is_err());
    }

    #[wasm_bindgen_test]
    fn validate_theme_reports_invalid_json_and_tokens_are_listed() {
        let out = validate_theme("not json");
        assert!(out.contains("\"valid\":false"));
        assert!(out.contains("invalid-json"));
        assert!(!token_names().is_empty());
    }
}
