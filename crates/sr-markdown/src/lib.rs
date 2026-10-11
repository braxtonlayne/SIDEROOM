//! sr-markdown: Discord-flavored markdown parser and AST (docs/02-architecture.md §3).
//!
//! The same crate runs on the server (mention extraction, search text, limits) and,
//! compiled to `wasm32-unknown-unknown`, in the browser. It is pure `std` with no I/O;
//! its only dependencies are `serde`/`serde_json` (AST JSON for the React renderer).
//!
//! WASM bindings live in the thin `sr-markdown-wasm` crate (built by `packages/wasm`).

mod ast;
mod block;
mod emoji;
mod inline;
mod util;

pub use ast::{LinkKind, Node, TimestampStyle};
pub use emoji::{
    JUMBO_MAX, count_emoji_only, emoji_only_count, expand_shortcodes, is_jumbo, shortcode_count,
    shortcode_to_emoji,
};
pub use util::{
    DEFAULT_MAX_CHARS, DEFAULT_MAX_MENTIONS, EXTENDED_MAX_CHARS, LimitError, Limits, Mentions,
    NoResolver, Resolver, char_count, check_limits, extract_mentions, to_plain_text,
    to_plain_text_with,
};

/// Parse message content into an AST. Never panics; unparseable syntax stays literal text.
pub fn parse(input: &str) -> Vec<Node> {
    block::parse_document(input)
}

/// Parse and serialize the AST to JSON for the React renderer.
pub fn parse_to_json(input: &str) -> String {
    serde_json::to_string(&parse(input)).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests;
