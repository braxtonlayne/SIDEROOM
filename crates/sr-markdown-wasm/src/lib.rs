//! Browser bindings for `sr-markdown`: the same parser, limits and mention extraction as
//! the server. Results are JSON strings; ids are decimal strings (snowflakes exceed
//! `Number.MAX_SAFE_INTEGER`). The typed facade lives in `packages/wasm/src`.

use sr_markdown::{Limits, Mentions, check_limits, extract_mentions, parse, to_plain_text};
use wasm_bindgen::prelude::*;

fn ids(v: &[u64]) -> Vec<String> {
    v.iter().map(u64::to_string).collect()
}

fn mentions_json(m: &Mentions) -> serde_json::Value {
    serde_json::json!({
        "users": ids(&m.users),
        "roles": ids(&m.roles),
        "channels": ids(&m.channels),
        "emoji": ids(&m.emoji),
        "everyone": m.everyone,
        "here": m.here,
    })
}

/// Mentions found in `content` as JSON.
pub fn mentions_of(content: &str) -> String {
    mentions_json(&extract_mentions(&parse(content))).to_string()
}

/// Check content against limits. `Ok` carries the mentions JSON; `Err` the reason.
pub fn check(content: &str, max_chars: usize, max_mentions: usize) -> Result<String, String> {
    check_limits(
        content,
        &Limits {
            max_chars,
            max_mentions,
        },
    )
    .map(|m| mentions_json(&m).to_string())
    .map_err(|e| e.to_string())
}

/// Parse message content to the AST JSON consumed by the React renderer.
#[wasm_bindgen(js_name = parseMarkdown)]
pub fn parse_markdown(input: &str) -> String {
    sr_markdown::parse_to_json(input)
}

/// Plain text for previews and notifications.
#[wasm_bindgen(js_name = toPlainText)]
pub fn plain_text(input: &str) -> String {
    to_plain_text(&parse(input))
}

/// Mentions in `content` as JSON: `{ users, roles, channels, emoji, everyone, here }`.
#[wasm_bindgen(js_name = extractMentions)]
pub fn extract_mentions_js(content: &str) -> String {
    mentions_of(content)
}

/// Validate length and mention count. Returns the mentions JSON, or throws.
#[wasm_bindgen(js_name = checkLimits)]
pub fn check_limits_js(
    content: &str,
    max_chars: usize,
    max_mentions: usize,
) -> Result<String, JsError> {
    check(content, max_chars, max_mentions).map_err(|e| JsError::new(&e))
}

/// Default message length limit.
#[wasm_bindgen(js_name = defaultMaxChars)]
pub fn default_max_chars() -> usize {
    sr_markdown::DEFAULT_MAX_CHARS
}

/// Default mention limit.
#[wasm_bindgen(js_name = defaultMaxMentions)]
pub fn default_max_mentions() -> usize {
    sr_markdown::DEFAULT_MAX_MENTIONS
}

/// Replace `:shortcode:` names with unicode emoji. The composer calls this at send time (Q89);
/// the parser itself leaves shortcodes literal.
#[wasm_bindgen(js_name = expandShortcodes)]
pub fn expand_shortcodes_js(input: &str) -> String {
    sr_markdown::expand_shortcodes(input)
}

/// True when the message renders with jumbo emoji (the same rule the server applies).
#[wasm_bindgen(js_name = isJumbo)]
pub fn is_jumbo_js(input: &str) -> bool {
    sr_markdown::is_jumbo(&parse(input))
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    /// The `JsError` path is only constructible on the wasm target.
    #[wasm_bindgen_test]
    fn check_limits_throws_js_error() {
        assert!(check_limits_js("abc", 2, 10).is_err());
        assert!(check_limits_js("abc", 3, 10).is_ok());
    }

    #[wasm_bindgen_test]
    fn parse_and_jumbo_round_trip() {
        assert!(is_jumbo_js(&expand_shortcodes_js(":smile:")));
        assert!(!is_jumbo_js(":smile:"));
        assert!(parse_markdown("**x**").contains("\"bold\""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_use_string_ids() {
        let out = mentions_of("hi <@123456789012345678> @everyone `<@5>`");
        assert!(out.contains("\"123456789012345678\""));
        assert!(out.contains("\"everyone\":true"));
        assert!(!out.contains("\"5\""));
    }

    #[test]
    fn limits_enforced() {
        assert!(check("abc", 2, 10).is_err());
        assert!(check("abc", 3, 10).is_ok());
    }

    #[test]
    fn parse_matches_core() {
        assert_eq!(parse_markdown("**x**"), sr_markdown::parse_to_json("**x**"));
        assert_eq!(plain_text("**x**"), "x");
    }
}
